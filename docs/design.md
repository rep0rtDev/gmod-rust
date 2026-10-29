# gmod-rust: дизайн-документ (GMod 1 в 1 на Rust, модель Horizon)

Имя проекта: **gmod-rust**. Ниже везде "gmod-rust" = наш движок и игра, "GMod" = оригинальный Garry's Mod на Source 1.

Версия документа: 0.2, 2026-10-28. Статус: предложение к обсуждению, все числа с пометкой "GMod" взяты из публичной документации (VDC, GMod wiki, SDK 2013) и подлежат подтверждению харнессами паритета (раздел 4).

---

## 0. Что это и что это не

**Это:** новая игра, которая для игрока неотличима от GMod sandbox: те же карты (gm_construct, gm_flatgrass, HL2-карты), те же пропы, оружие HL2, физган, тулган, спавн-меню, та же физика и движение, тот же HUD. Весь код движка и геймплея написан заново на Rust, многопоточно, под Vulkan/Metal/DX12 через wgpu.

**Это не:**
- не порт и не реимплементация Source (не грузим `engine.dll`, не читаем утечки, не линкуем SDK);
- не совместимо с GMod: не заходит на GMod-сервера, не запускает GLua-аддоны, не читает GMA из Workshop GMod;
- не поставляет контент: в репозитории и в релизах ни одного файла Valve/Facepunch. Контент берётся из Steam-копии GMod на машине игрока (как Horizon берёт Minecraft из официального лаунчера).

**Своя экосистема:** свой лаунчер, свой протокол, свои сервера, свой реестр аддонов, свой язык аддонов (Luau + WASM).

### 0.1 Принципы (копируем у Horizon, потому что они работают)

1. **Паритет измеряется.** Подсистема не считается готовой, пока её харнесс не сравнивает выход gmod-rust с выходом настоящего GMod на одинаковых входах и не сходится в допуске. Количество харнессов - метрика проекта.
2. **Ни байта чужого.** CI-проверка `check_public_tree` блокирует коммит любого файла с сигнатурой VPK/BSP/VTF/MDL/WAV из GMod, любого декомпилята и любых фрагментов из утечек. Нарушение = откат, как Horizon откатил шейдерпаки после инцидента с Iris.
3. **Источники знаний:** публичная документация (VDC wiki, GMod wiki), собственные измерения на реальном GMod, Source SDK 2013 и Lua-база GMod **как справочник поведения**, и реверс закрытых бинарей GMod там, где иначе никак (раздел 10). Ни SDK, ни декомпилят не являются источником кода: спецификация пишется по ним отдельно от реализации, реализация пишется по спецификации и харнессам.
4. **Код пишут агенты, проверяет машина.** Как у Horizon: большинство кода - ИИ-сессии, каждый PR проходит `verify` (fmt, clippy с `-D warnings`, тесты, check_public_tree) и `parity` (харнессы, затронутые изменением) на машине с установленным GMod. Человек решает что делать, не читает каждую строку.
5. **Многопоточность по умолчанию.** Нет "главного потока, который делает всё". Симуляция, рендер, физика, аудио, сеть, стриминг ассетов и аддоны - отдельные потоки/пулы с явными точками синхронизации (раздел 2.2).

---

## 1. Контент: что монтируем и как

### 1.1 Что лежит в Steam-копии GMod

Путь: `<SteamLibrary>/steamapps/common/GarrysMod/`. Нужное:

| Папка / файл | Формат | Что внутри | Нужно для |
|---|---|---|---|
| `garrysmod/garrysmod_dir.vpk` + `garrysmod_000..NNN.vpk` | VPK v2 | GMod-собственные материалы, модели (физган, тулган, камера), звуки, шрифты, `settings/spawnlist/*.txt` | ядро GMod |
| `sourceengine/hl2_textures_dir.vpk`, `hl2_misc_dir.vpk`, `hl2_sound_*_dir.vpk` | VPK v2 | HL2-контент: пропы, оружие, NPC, звуки | 90% того, что игрок видит |
| `garrysmod/maps/*.bsp` | BSP v20/21 | gm_construct, gm_flatgrass + карты из `sourceengine/` для HL2 | карты |
| `garrysmod/materials/`, `models/`, `sound/` (рассыпные) | VMT/VTF/MDL/WAV | то, что не в VPK | дополнения |
| `garrysmod/scripts/` | KeyValues | `surfaceproperties.txt`, `game_sounds_*.txt`, `soundscapes_*.txt`, `propdata.txt`, `weapon_*.txt`, `hudlayout.res`, `hudanimations.txt` | физика материалов, звуковые скрипты, HUD |
| `garrysmod/resource/` | RES/TTF | шрифты, схемы VGUI (`ClientScheme.res`), локализация `gameui_*.txt`, `garrysmod_*.txt` | UI 1 в 1 |
| `garrysmod/gamemodes/sandbox/` | Lua + txt | **читаем только данные**: `spawnmenu`, спавнлисты, настройки; Lua-код не исполняем | спавн-меню, список инструментов |
| `garrysmod/cfg/` | CFG | дефолты консольных переменных | дефолты |

Монтируем в порядке GMod (`gameinfo.txt` -> `SearchPaths`): `garrysmod/` поверх `sourceengine/` поверх `platform/`. Коллизии имён решаются в этом порядке. Это даёт точно те же файлы, которые видит GMod.

### 1.2 Пайплайн ресурсов (лаунчер, одноразово + инкрементально)

Horizon не читает Java-ресурсы на лету, он один раз готовит из них свои. Мы делаем так же, потому что Source-форматы не предназначены для GPU напрямую и парсятся медленно.

| Исходник | Что делаем | Результат (кэш, формат наш) | Оценка объёма |
|---|---|---|---|
| VTF (DXT1/DXT5/BGRA8888/RGBA16161616F, мипы, кубмапы, анимации) | транскодируем в BC1/BC3/BC6H/BC7 (`basis`/`intel_tex`), мипы сохраняем | KTX2 с supercompression zstd | HL2+GMod ~2.5 ГБ VTF -> ~1.8 ГБ KTX2 |
| VMT (KeyValues) | парсим, резолвим `$basetexture`, `$bumpmap`, `$envmap`, `$detail`, `$translucent`, `$alphatest`, `$nocull`, `$surfaceprop`, прокси (`AnimatedTexture`, `Sine`, `TextureScroll`) | бинарный материал (наш `.rmat`, serde/rkyv) | ~45 000 материалов |
| MDL/VVD/VTX/PHY/ANI (v44-v49) | кости (до 128 в Source), bodygroups, skins, LOD, hitboxes, attachments, последовательности, flex-ы, физ-хуллы | `.rmdl` с интерливленными вершинами под wgpu + отдельно `.ranim` | ~9 000 моделей |
| BSP v20/21 | лампы: PLANES, FACES, LIGHTING + LIGHTING_HDR, DISP_INFO, VISIBILITY (PVS), LEAF_AMBIENT_LIGHTING, CUBEMAPS, GAME_LUMP sprp (static props), PAKFILE (встроенные материалы), ENTITIES (KeyValues); строим меши по материалам, лайтмап-атлас, BVH для коллизий | `.rmap` | gm_construct ~15 с конверсии на 8 ядрах (цель) |
| WAV/MP3 | WAV: PCM 8/16 бит, ADPCM; cue-точки (loop) | как есть или `.ogg`/`.wav` без изменений | ~1.5 ГБ |
| `scripts/*.txt`, `resource/*.res` | KeyValues -> наши структуры | JSON/`.rkv` | мало |

Кэш лежит в `%LOCALAPPDATA%/gmod-rust/cache/<hash gameinfo + версия пайплайна>/`. Сборка полного кэша: цель <= 10 минут на 8 ядрах, инкрементально по mtime/hash. Кэш не распространяется (это производная от контента Valve).

Проверка владения: лаунчер проверяет `steamapps/appmanifest_4000.acf` (StateFlags=4) и наличие `garrysmod_dir.vpk`. Без GMod игра не запускается, как Horizon без Minecraft.

### 1.3 Юридическая рамка (кратко, не юрконсультация)

- Код gmod-rust: MIT или Apache-2.0. В репе нет ни файлов Valve/Facepunch, ни их фрагментов (хеши, таблицы, декомпилят).
- Контент остаётся у игрока, мы его только читаем. Так работают OpenMW, Xash3D FWGS, Horizon.
- Source SDK 2013 читаем как документацию поведения. Правило clean-room: файл спецификации (`docs/spec/<система>.md`) пишется одним агентом по SDK и VDC, реализация пишется другим агентом только по спецификации и харнессам. В коде не должно быть идентификаторов SDK (`CGameMovement`, `mv->`, `sv_airaccelerate` как имя переменной и т.п.), только наши имена.
- Названия: не используем "Garry's Mod", "Source", "Half-Life" в имени продукта и логотипе; в описании - "требуется установленная копия Garry's Mod".
- Коммерция закрыта по определению (контент чужой). Монетизация возможна только вокруг экосистемы (хостинг серверов, реестр), это отдельный разговор.

---

## 2. Архитектура движка

### 2.1 Крейты (Cargo workspace)

```
gmod-rust/
  crates/
    gm-core        # математика (glam), единицы Source (1 unit = 0.75 inch = 1.905 см), время, id, ошибки
    gm-formats     # парсеры: vpk, bsp, vtf, vmt, mdl/vvd/vtx/phy/ani, keyvalues, wav. Чистые, без IO
    gm-content     # монтирование SearchPaths, виртуальная ФС, кэш, пайплайн конверсии (gm-formats -> наши форматы)
    gm-world       # BSP-мир: PVS, листья, области, порталы, трассировка хуллов, контент-флаги, I/O энтити
    gm-physics     # обёртка Jolt: тела, констрейнты, материалы (surfaceproperties), трассы, callbacks
    gm-move        # движение игрока: ground/air/water/ladder/crouch/jump/steps, детерминированно, f32
    gm-sim         # ECS (hecs или bevy_ecs отдельно, без Bevy App), тик 66.67 Гц, энтити GMod, события
    gm-net         # протокол, снапшоты, дельта, предикция/реконсиляция (lightyear/replicon или своё поверх quinn)
    gm-render      # wgpu: BSP-пасс, static props, модели со скелетом, лайтмапы, кубмапы, прозрачность, посты
    gm-ui          # 2D: панели, шрифты из resource/, схема VGUI, HUD, спавн-меню, контекст-меню
    gm-audio       # микшер, soundscripts, soundscapes, DSP; опционально Steam Audio (переиспользуем gmod-steamaudio)
    gm-script      # Luau-акторы (mlua, feature luau), API биндинги, планировщик фаз
    gm-wasm        # wasmtime component model, WIT-мир аддонов, capabilities
    gm-game        # sandbox: физган, тулган, инструменты, спавн-меню, лимиты, HL2 оружие/NPC/транспорт
    gm-server      # dedicated server (headless): gm-sim + gm-net + gm-physics + gm-script, без рендера
    gm-client      # окно (winit), ввод, рендер, UI, предикция
  apps/
    launcher       # лаунчер (Rust + egui или iced), обновления, кэш ресурсов, серверный браузер, аддоны
    server         # бинарь сервера
    client         # бинарь клиента
  tools/
    parity/        # харнессы (раздел 4): probe для GMod (Lua + бинарный модуль), runner, сравнение
    check_public_tree.py
  docs/spec/       # спецификации поведения по подсистемам (clean-room слой)
```

Зависимости (все с большой историей и активным сопровождением): `wgpu`, `winit`, `glam`, `hecs`/`bevy_ecs`, `joltc-sys`/`rolt` (Jolt 5.x), `mlua` (Luau), `wasmtime` (component-model), `quinn` (QUIC) или `lightyear` 0.31 + `aeronet`, `binrw` для форматов, `rkyv`/`postcard` для кэша и сети, `rayon` для пулов, `tracing` + `tracy` для профилирования.

### 2.2 Потоки и фазы

Целевая машина: 8 ядер / 16 потоков. Ничего не привязано к одному ядру.

| Поток / пул | Частота | Делает | Синхронизация |
|---|---|---|---|
| **Sim** (1 поток) | 66.67 Гц (тик 15 мс, как GMod по умолчанию) | тик мира: ввод игроков -> движение -> запуск физики -> пост-физика -> события -> снапшот | владеет ECS; раздаёт read-only снапшот |
| **Physics jobs** (Jolt, N-2 потоков) | внутри тика | broad/narrow phase, острова, констрейнты | `PhysicsSystem::Update` с нашим JobSystem на rayon; Sim ждёт завершения |
| **Script actors** (пул N-2) | внутри тика, параллельная фаза | Luau-акторы читают снапшот, считают, ставят записи в очередь | серийная фаза применяет очереди в порядке приоритета |
| **Render** (1 поток + wgpu-очереди) | без привязки к тику, цель 240+ FPS | кулинг по PVS, сбор draw-list, запись командных буферов | берёт последние 2 состояния Sim, интерполирует |
| **Asset streaming** (пул 2-4) | по запросу | чтение кэша, загрузка в GPU, декомпрессия zstd | каналы crossbeam |
| **Audio** (1 поток RT) | 48 кГц, буфер 256-512 сэмплов | микшер, DSP, spatialization | SPSC-кольца (как в gmod-steamaudio) |
| **Net** (1 поток) | по сокету | QUIC/UDP приём-отправка, шифрование | каналы |

Бюджет тика на сервере с 64 игроками и 2 000 активных физ-тел: <= 6 мс из 15 (движение 0.5, физика 3.0, скрипты 1.5, снапшоты 1.0). Это измеряемая цель, не обещание.

Детерминизм: `gm-move` и всё, что предиктится на клиенте, пишется в `f32` в фиксированном порядке операций, без `fast-math`, одинаково на клиенте и сервере (одна функция). Физика Jolt детерминирована на одной платформе/компиляторе; кросс-платформенный детерминизм не обещаем, поэтому пропы не предиктятся, а интерполируются (как в GMod).

### 2.3 ECS и энтити

Энтити 1 в 1 по классам GMod/HL2, которые видит игрок: `player`, `prop_physics`, `prop_ragdoll`, `prop_dynamic`, `prop_static` (из BSP), `func_*` (brush-энтити: `func_door`, `func_button`, `func_breakable`, `func_movelinear`, `trigger_*`), `light*` (запечены, но нужны для `env_*`), `env_*` (`env_sprite`, `env_explosion`, `env_fire`), `weapon_*` HL2, `npc_*` HL2, `prop_vehicle_jeep`/`airboat`, `gmod_*` (gmod_hands, gmod_camera, gmod_tool, gmod_light, gmod_lamp, gmod_balloon, gmod_thruster, gmod_wheel, gmod_hoverball, gmod_button, gmod_dynamite, gmod_emitter, gmod_wire_* не нужно).

Компоненты: Transform (pos f32x3, angles pitch/yaw/roll как в Source), Velocity, Model (handle, skin, bodygroups, sequence, cycle, flexes), PhysBody (Jolt BodyID, mass, material, motion type, frozen), Collision (group, solid flags), Health, Owner, NetId, Parent (attachment), Color/RenderMode/RenderFX, Name/Targetname + I/O (Source entity I/O: выходы `OnPressed` -> входы `Open`, задержки, параметры), PlayerState (раздел 3.1).

Лимит энтити: 8 192 (GMod поднял лимит edicts с 2 048 Source до 8 192; наш протокол без этого ограничения, но лимит оставляем конфигурируемым, чтобы поведение "достигнут лимит" совпадало).

---

## 3. Геймплейные системы: паритет по числам

Всё в юнитах Source (1 unit = 1.905 см). Значения "GMod" - дефолты из публичной документации; каждое подтверждается харнессом, расхождение фиксируется в `docs/spec/<система>.md`.

### 3.1 Движение игрока (`gm-move`)

| Параметр | GMod по умолчанию | Примечание |
|---|---|---|
| Тик | 66.67 Гц (0.015 с) | движение считается на тике, рендер интерполирует |
| Хулл стоя | 32 x 32 x 72, глаза на 64 | `VEC_HULL_MIN/MAX`, `VEC_VIEW` |
| Хулл сидя | 32 x 32 x 36, глаза на 28 | переход 0.4 с вниз (`duck` анимация), вверх мгновенно если есть место |
| Гравитация | 800 u/s^2 | `sv_gravity` |
| Скорость ходьбы / бега | 200 / 400 u/s | sandbox: `Player:SetWalkSpeed(200)`, `SetRunSpeed(400)` |
| Сидя | 0.3 от скорости | `CrouchedWalkSpeed` |
| Прыжок | 200 u/s вверх (`JumpPower`) | sandbox дефолт; bhop не режется (нет `sv_enablebunnyhopping`-ограничений в sandbox) |
| Ускорение на земле | 10 (`sv_accelerate`) | `accel = accelerate * wishspeed * dt`, cap до `wishspeed - currentspeed` |
| Ускорение в воздухе | 10 (`sv_airaccelerate`), wishspeed cap 30 | даёт airstrafe как в Source |
| Трение | 4 (`sv_friction`), `sv_stopspeed` 100 | `control = max(speed, stopspeed); drop = control * friction * dt` |
| Шаг | 18 u (`sv_stepsize`) | step-up со сдвигом, затем проверка земли |
| Максимальная скорость | 3 500 u/s (`sv_maxvelocity`) по компонентам | |
| Лестницы | `func_ladder`/`func_useableladder`, скорость 200 вдоль | |
| Вода | уровни 0-3 по хуллу, плавание 2 u/s^2 вверх, трение воды, `sv_waterdist` 12 | |
| Падение | урон от скорости > 580 u/s (`PLAYER_MAX_SAFE_FALL_SPEED`), смерть ~1 024; формула линейная до 100 HP | sandbox: `mp_falldamage 0` -> фикс 10 HP |
| Noclip | sandbox по умолчанию разрешён, скорость 400/1000 (шифт), без коллизий | как GMod |
| Вид | pitch ограничен [-89, 89], view roll от скорости (`cl_rollangle` 0 в GMod) | |
| Боб оружия | `cl_bob*` - HL2 bob, частота 0.8, амплитуда 0.01 | в viewmodel |

Входы: `usercmd {tick, buttons bitmask (attack, jump, duck, forward, back, moveleft, moveright, use, speed, walk, reload, attack2, zoom, score), viewangles, forwardmove/sidemove/upmove (f32), weapon select, impulse}`. Клиент отправляет историю последних 8 команд в каждом пакете (защита от потерь), сервер исполняет команду N на тике N (как Source и как lightyear).

Паритет: харнесс `move_*` (раздел 4.2) гоняет записанные последовательности usercmd и сравнивает позицию/скорость на каждом тике. Допуск: 1e-3 u по позиции после 1 000 тиков на ровной геометрии, 0.05 u на лестницах/дисплейсментах.

### 3.2 Физика (`gm-physics`, Jolt)

| Что | GMod (VPhysics поверх IVP, Ipion Virtual Physics; в символах `vphysics_srv.so` видны `IVP_Real_Object`, `CreateShadowController`, `ComputeShadowControllerHL`) | gmod-rust (Jolt) |
|---|---|---|
| Формы | convex hulls из `.phy` (до 16 на модель у пропов), brush-энтити = набор convex, мир = BSP-полигоны | те же `.phy` хуллы -> `ConvexHullShape`/`StaticCompoundShape`; мир -> `MeshShape` с BVH |
| Масса | из `.phy` (`$mass`) или плотность `surfaceproperties` x объём | то же, та же таблица |
| Материалы | `surfaceproperties.txt`: friction, elasticity, density, dampening, thickness, звуки удара/скольжения | парсим ту же таблицу, маппим на Jolt `PhysicsMaterial`; звуки по тем же порогам скорости удара (VPhysics: impact soft > 100 u/s, hard > 300 u/s - уточнить) |
| Гравитация | `sv_gravity` 800 u/s^2 и для игрока, и для VPhysics-тел (проверить: у тел возможен свой множитель) | **харнесс** `phys_drop_crate_1m`: измеряем падение пропа и берём то, что покажет GMod |
| Шаг симуляции | VPhysics ~ 1/66 с внутри тика, субшаги | Jolt `Update(dt=0.015, collisionSteps=2)`; при тонких объектах 4 |
| Сон | тело засыпает при скорости < 0.1 u/s ~1 с | Jolt sleep thresholds подгоняем по харнессу "стопка 10 ящиков успокаивается за T с" |
| Констрейнты | weld, axis, ballsocket, rope, slider, elastic, hydraulic, motor, pulley, winch, keepupright, nocollide | Fixed, Hinge, Point, Distance(min/max) + rope render, Slider, SpringDistance, Hinge+motor, Pulley, Distance с мотором, SwingTwist/custom, collision-filter |
| Прочность констрейнтов | `forcelimit`/`torquelimit` в инструментах | Jolt break: проверка реакции на тике, рвём при превышении |
| Рэгдоллы | `.phy` с несколькими солидами + ragdoll constraints | Jolt `Ragdoll` + `SwingTwistConstraint` по тем же лимитам из `.phy` |
| Производительность | GMod: сервер начинает лагать ~500-1000 активных пропов | цель gmod-rust: 5 000 активных тел при <= 3 мс/тик на 8 ядрах |

Поведение, которое игрок почувствует и которое надо воспроизвести именно харнессами, а не "на вид": отскок пропов от стен (elasticity), трение при скольжении ящика по бетону, устойчивость стопок, прыжки на пропах (игрок стоит на физ-объекте: Source двигает игрока с объектом через `groundentity`), падение рэгдолла с лестницы, прокидывание тонких предметов сквозь стены (в Source это баг; воспроизводить баги не будем, фиксируем как "осознанное расхождение").

### 3.3 Физган (`weapon_physgun`)

| Параметр | GMod |
|---|---|
| Дальность захвата | трассировка 4 096 u (`physgun_maxrange`) |
| Захват | ЛКМ: трасса -> `PhysObj` -> точка на объекте сохраняется в локальных координатах объекта |
| Удержание | объект тянется к точке "луч x дистанция" через shadow-контроллер: `max speed 2048 u/s`, `max angular 1024 deg/s`, damping, teleport при дистанции > 32 u? (харнесс) |
| Колесо мыши | меняет дистанцию, шаг ~ 50 u (харнесс) |
| ПКМ при удержании | заморозка (motion disabled, остаётся где есть), снятие заморозки: `R` на объекте или релоад снимает все заморозки игрока |
| Shift + мышь | вращение объекта вокруг осей вида, `E` снап к 45 градусам |
| Луч | материал `sprites/physbeam`, цвет из настроек игрока (`cl_weaponcolor`), свечение |
| Ограничения | нельзя взять игроков без `sbox_playershurtplayers`/админ-права; `physgun_limited`; нельзя мир, статик-пропы, хотя `prop_dynamic` можно если есть PhysObj |
| Эффекты | `physgun_halo`, искры при заморозке, звук `weapons/physcannon/physcannon_charge1.wav`-семейство из HL2 + GMod `physgun` звуки |

Это самое заметное "ощущение GMod". Харнесс `physgun_*` записывает траекторию удерживаемого пропа при фиксированном движении мыши (раздел 4).

### 3.4 Тулган и инструменты (`gmod_tool`)

Тулган - оружие с режимом `tool`, выбранным в спавн-меню. Инструменты GMod base (все, с параметрами из их `ConVar`-ов, значения по умолчанию из `garrysmod/lua/weapons/gmod_tool/stools/*.lua` читаем **как данные** для спека, не исполняем):

Constraints: `weld` (forcelimit 0, nocollide), `axis`, `ballsocket`, `ballsocket_adv`, `rope` (длина, материал `cable/rope`, ширина 1-3), `elastic`, `hydraulic`, `muscle`, `motor`, `pulley`, `slider`, `winch`, `wheel`, `nocollide`, `keepupright`.
Construction: `remover`, `duplicator` (копирует энтити+констрейнты в дерево; наш формат `.rdup`, не `.dupe`), `material`, `colour`, `paint`, `trails`, `eyeposer`, `faceposer`, `finger`, `inflator`, `light`, `lamp`, `balloon`, `button`, `dynamite`, `emitter`, `hoverball`, `thruster`, `camera`, `physprop` (gravity on/off, material), `ignite`, `creator`, `leafblower`? (нет, это не base), `example`.

Каждый инструмент: `LeftClick`, `RightClick`, `Reload` с трассой 4 096 u? (у тулгана дальность трассы `tool_trace` - харнесс), ghost-энтити (полупрозрачный предпросмотр), контекстные настройки (`ControlPanel`), лимиты `sbox_max<thing>`.

Лимиты sandbox по умолчанию (GMod `cfg/sandbox.cfg`... харнесс выписывает актуальные): `sbox_maxprops`, `sbox_maxragdolls`, `sbox_maxnpcs`, `sbox_maxvehicles`, `sbox_maxballoons`, `sbox_maxeffects`, `sbox_maxdynamite`, `sbox_maxlamps`, `sbox_maxthrusters`, `sbox_maxwheels`, `sbox_maxhoverballs`, `sbox_maxbuttons`, `sbox_maxemitters`, `sbox_maxlights`, `sbox_maxcameras`, `sbox_maxsents`, `sbox_godmode`, `sbox_noclip`, `sbox_playershurtplayers`, `sbox_weapons`, `sbox_bonemanip_*`. HUD показывает счётчики как GMod (верхний левый угол).

### 3.5 Спавн-меню и контекстное меню

- `Q` (hold): спавн-меню 1 в 1 по разметке GMod: вкладки `Spawnlists` (дерево из `settings/spawnlist/*.txt` + `Browse` по смонтированным папкам `models/`), `Weapons`, `Entities`, `NPCs`, `Vehicles`, `Post Process`, `Dupes`, `Saves`; правая панель `Tools` с категориями и `Options`/`Utilities`.
- Иконки пропов: GMod рендерит `spawnicons` в кэш `materials/spawnicons/*.png`. Мы рендерим сами в фоне (wgpu offscreen 128x128, тот же угол камеры, что в GMod: автофрейминг по bounding box), кэшируем в нашем кэше. У тех, у кого иконки уже есть в GMod-папке, можно читать их (быстрее первого запуска).
- Спавн пропа: трасса от глаз 4 096 u? (харнесс), позиция = точка попадания + подъём по OBB так, чтобы объект стоял на поверхности (GMod: `DoPropSpawnedEffect`, эффект появления `propspawn`), ориентация: yaw игрока + 180.
- `C` (hold): контекстное меню: свойства энтити по ПКМ (`Remove`, `Ignite`, `Color`, `Material`, `Bodygroups`, `Skin`, `Collision`, `Gravity`, `Keep Upright`, `Make Persistent`, `Edit properties` для ламп/света и т.д.), настройка цвета игрока/оружия, пост-процессы.
- Undo: `Z` отменяет последнее действие игрока (стек на игрока, лимит 100? харнесс), текст "Undone Prop" в HUD, звук `buttons/button15.wav`.
- Клинап: `Utilities -> Cleanup`, админский глобальный.

### 3.6 Оружие HL2 и GMod

`weapon_crowbar`, `weapon_stunstick`, `weapon_pistol`, `weapon_357`, `weapon_smg1` (+ гранаты alt), `weapon_ar2` (+ шар), `weapon_shotgun`, `weapon_crossbow`, `weapon_frag`, `weapon_rpg` (лазерное наведение), `weapon_slam`, `weapon_bugbait`, `weapon_physcannon` (гравипушка, отдельная механика: punt/pull, дальность 250/`physcannon_tracelength` 250, `physcannon_pullforce` 4000, `physcannon_maxforce` 1500, `physcannon_cone` 0.97, `physcannon_ball_cone` 0.997, мегапушка нет), `weapon_physgun`, `gmod_tool`, `gmod_camera`, `weapon_medkit`, `weapon_fists`, `weapon_flechettegun`? (нет, это не база GMod; есть `manhack_welder` нет). Для каждого: скрипт `scripts/weapon_*.txt` (clip, ammo type, viewmodel, worldmodel, HUD иконки из шрифтов `HalfLife2.ttf`), spread, damage (`sk_plr_dmg_*` дефолты), fire rate, анимации viewmodel (из `.mdl`, последовательности `idle`, `fire`, `reload`, `draw`), звуки из `game_sounds_weapons.txt`. Урон по хитбоксам с множителями (`sk_player_head` 3.0, chest 1.0, stomach 1.0, arm 1.0, leg 1.0 - харнесс).

Боеприпасы: таблица `ammo_*` типов с `max carry` (Pistol 150, SMG1 225, AR2 60, Buckshot 30, 357 12, XBowBolt 10, RPG_Round 3, Grenade 5, slam 5, AR2AltFire 3, SMG1_Grenade 3 - сверить).

### 3.7 NPC HL2 и транспорт (длинный хвост)

NPC: `npc_citizen`, `npc_combine_s`, `npc_metropolice`, `npc_zombie`, `npc_fastzombie`, `npc_poisonzombie`, `npc_headcrab*`, `npc_antlion`, `npc_antlionguard`, `npc_manhack`, `npc_rollermine`, `npc_barnacle`, `npc_alyx`, `npc_barney`, `npc_kleiner`, `npc_gman`, `npc_strider`, `npc_hunter` (EP2 - есть в GMod), `npc_helicopter`, `npc_combinegunship`, `npc_turret_floor`, `npc_vortigaunt`, `npc_dog`, `npc_monk`, `npc_eli`, `npc_mossman`, `npc_breen`, `npc_stalker`, `npc_crow/pigeon/seagull`.

HL2 AI - это схедулы/задачи/условия, nodegraph (`.ain` файлы в `maps/graphs/`, GMod их генерирует при первом запуске карты), squad-ы, hint-ноды, укрытия, боевые дистанции на класс, отношения (`npc_citizen` vs `npc_combine_s`), анимационные активити (`ACT_RUN`, `ACT_IDLE_ANGRY` и т.д. через `.mdl` sequences с activity-маппингом), речь (`npc_sentences.txt`, `scenes/*.vcd` для лицевой анимации).

Это самая большая и самая плохо документированная часть. План: в v1 NPC как **"манекены с боевым контуром"** (ходят по navmesh, стреляют, умирают, рэгдолл, звуки), поведение сверяем харнессами по агрегатам (время реакции, точность, дистанции), а не по траекториям. Полный паритет AI - отдельный многолетний трек после v1. Navmesh: читаем `.ain` как данные, если он есть, иначе строим свой (recast через `recastnavigation-rs`).

Транспорт: `prop_vehicle_jeep` (Jeep и Airboat через `vehicle scripts/*.txt`: мощность, передачи, подвеска, колёса как raycast-wheels). Jolt имеет `WheeledVehicleConstraint`, параметры подгоняем по харнессу "разгон 0-max за T с, тормозной путь, максимальная скорость".

### 3.8 UI и HUD (`gm-ui`)

- Пишем свой 2D-слой поверх wgpu (не egui для внутриигрового UI: нужен точный вид GMod). Шрифты из `resource/*.ttf` + `ClientScheme.res` (размеры, антиалиасинг, outline, additive) через `fontdue`/`swash`.
- HUD GMod sandbox: здоровье/броня в нижнем левом (цвета GMod: `Color(255,80,0)` health bar? харнесс-скриншот), боеприпасы справа, счётчики лимитов сверху слева, подсказка оружия, кроссхейр (точка/крест по оружию), "Undone X" уведомления, чат (Y), голосовые индикаторы, список игроков по TAB (scoreboard: аватары Steam, ник, пинг, deaths).
- Derma-подобные панели для спавн-меню/контекст-меню: `Frame`, `Panel`, `Button`, `Label`, `TextEntry`, `Slider` (NumSlider), `CheckBox`, `ComboBox`, `Tree`, `ListView`, `ScrollPanel`, `PropertySheet` (вкладки), `ColorMixer`, `ModelPanel` (рендер модели в панели), `SpawnIcon`, `ControlPanel` (инструменты). Скины: дефолтный GMod `Default` (цвета/отступы 1 в 1 по скриншотам, харнесс SSIM).
- Главное меню: у GMod это HTML (Awesomium/Chromium). Мы делаем своё меню тем же 2D-слоем, визуально приближенное, без HTML: это осознанное расхождение (меню - часть лаунчера/экосистемы, не игры).
- Консоль (`~`): наша, с автодополнением, тем же шрифтом.

### 3.9 Рендер (`gm-render`)

Цель: "как GMod на максимальных", плюс современные правки, которые не меняют вид (TAA выключен по умолчанию, чтобы картинка совпадала; есть опция).

| Элемент Source | Как у нас |
|---|---|
| BSP-мир с лайтмапами (LDR+HDR), lightstyles (мигающие лампы) | лайтмап-атлас 4k, lightstyles как в Source (строки `a`-`z`, 10 Гц), HDR-лампы + тонмаппинг `mat_tonemap` аналог (авто-экспозиция с теми же границами `mat_autoexposure_min/max` 0.5/2.0) |
| `LightmappedGeneric`, `WorldVertexTransition` (blend дисплейсментов), `UnlitGeneric`, `VertexLitGeneric` (модели: ambient cube из `LEAF_AMBIENT_LIGHTING` + до 4 локальных источников, Phong, rimlight, `$envmap` кубмапы из BSP, `$detail`, `$selfillum`), `Water` (отражение/рефракция, `$fogcolor`), `Refract`, `Sky` (skybox 6 VTF + 3D skybox со `sky_camera` масштаб 1/16), `Cable`, `SpriteCard`/`Sprite` (частицы, `env_sprite`, физган-луч), `Decal`/`DecalModulate`, `Eyes`/`EyeRefract`, `Teeth` | по одному wgpu-шейдеру (WGSL) на семейство, параметры VMT -> uniform-блок; прокси (`AnimatedTexture`, `TextureScroll`, `Sine`, `PlayerColor` для цвета игрока) исполняются на CPU per-material |
| Статик-пропы с запечённым освещением (`sprp` lump, per-vertex lighting в `sp_*.vhv`) | instancing по модели, лод по `screen size` как `r_staticpropinfo` |
| Динамические тени (`r_shadows`, blobby shadow под моделями, `r_shadowrendertotexture`) + проекционные тени от `env_projectedtexture` (фонарик игрока: `flashlight`, 1 на игрока) | теневые карты для флэшлайта/лампы (`gmod_lamp`), blob-тени для моделей как в GMod |
| Частицы: `.pcf` (Source Particle System) для эффектов HL2/GMod (взрывы, кровь, дым) | парсер PCF (DMX binary v1/v2) + симулятор операторов (ограниченный набор ~40 операторов, которые реально используются в HL2/GMod эффектах; остальное - по мере харнессов) |
| Пост-процесс: bloom (`mat_bloomscale`), color correction (`.raw` LUT из `color_correction` энтити), motion blur (`mat_motion_blur_enabled` 0 по умолчанию), GMod `Post Process` вкладка (bokeh DOF, sharpen, sobel, bloom, texturize, sunbeams, toytown, overlay, super DOF, FXAA) | 1 в 1 по эффекту, параметры из той же вкладки |
| Декали (пули, кровь) `infodecal`, рантайм-декали на BSP и моделях, лимит `r_decals` 2 048 | то же, лимит конфигурируемый |
| Vgui-мир (`vgui_screen`)/HTML-панели | нет в v1 |

Производительность: gm_construct на RTX 3060 в 1440p: цель >= 500 FPS без постов (GMod ~300-400 с одним ядром на драйвер DX9). Профилируем через `tracy`.

### 3.10 Аудио (`gm-audio`)

- Soundscripts (`game_sounds_*.txt`): `channel`, `volume`, `pitch` (диапазоны), `soundlevel` (SNDLVL_* -> дистанционное затухание как в Source: `dist_mult = (SNDLVL - 60)/... `, таблица из VDC), `rndwave` варианты.
- Soundscapes (`soundscapes_*.txt`): `env_soundscape` триггеры по позиции игрока, `playlooping`, `playrandom` с интервалами, `dsp`.
- DSP-пресеты Source (`dsp_presets.txt`, room/player/water) - реализуем те же номера эффектов, что реально используются картами GMod/HL2 (room 0-29 + water 14/15 + facing).
- Пространственность: по умолчанию как в Source (панорамирование + затухание). Опция: Steam Audio (у нас уже есть реализация в `gmod-steamaudio`: BSP-геометрия, surfaceprops -> акустические материалы, окклюзия, HRTF). Это **наше** код-преимущество над GMod, переносится как есть (C++ -> Rust FFI к phonon).
- Голосовой чат: Opus 24 кГц, 3D-позиционирование по игроку, `voice_loopback`, индикаторы в HUD.
- Шаги: `surfaceproperties` -> `stepleft/stepright` звуки, интервалы по скорости (Source: 400 мс при ходьбе, 300 при бегe - харнесс), приземление.

---

## 4. Харнессы паритета (главный инструмент проекта)

### 4.1 Устройство

Horizon сравнивает Java-оракул и Rust-пробу на одинаковых входах. У нас оракул - реальный GMod на машине CI (Windows или Linux x64 клиент, Steam app 4000; на Linux dedicated server app 4020 для серверных харнессов).

```
tools/parity/
  probe/            # то, что живёт внутри GMod
    lua/            # аддон-пробник: autorun/parity_probe.lua, читает сценарии из data/, пишет трассы в data/parity/*.json
    native/         # бинарный модуль (gmcl_parity_*.dll/.so) для того, чего из Lua не достать: точные тайминги, скриншоты буфера, аудиобуфер
  scenarios/        # входы: usercmd-последовательности, спавн-скрипты, камеры
  runner/           # Rust: поднимает GMod (`-console -novid +map gm_construct +exec parity.cfg`), ждёт готовности, запускает сценарий, собирает трассу
  compare/          # сравнение трасс с допусками, отчёт, "golden" кэш трасс GMod (не контент, наши измерения; хранить можно)
  harnesses.toml    # список: имя, сценарий, допуск, какие крейты его затрагивают (для выборочного запуска в PR)
```

Пробник в GMod пишется на GLua (это единственное место в проекте, где GLua есть; живёт только в `tools/`, в игру не попадает). Он: читает сценарий, подменяет ввод (`CreateMove` на клиенте / `SetupMove` на сервере), каждый тик записывает `Player:GetPos()`, `GetVelocity()`, `GetMoveType()`, `OnGround`, `WaterLevel`, `EyeAngles`, для пропов `PhysObj:GetPos/GetAngles/GetVelocity/GetAngleVelocity/IsAsleep`, события (`PhysicsCollide`, звуки через `EntityEmitSound`), а также один раз - все ConVar-дефолты, `surfaceproperties`, лимиты sandbox, таблицы боезапаса (через `game.GetAmmoTypes`), спавнлисты.

Трассы GMod кэшируются по хэшу (сценарий + версия GMod + карта). Их можно хранить в репе: это наши измерения, не контент Valve.

Сторона gmod-rust: `gm-server`/`gm-client` в режиме `--parity <scenario>`: детерминированный прогон без рендера (или с offscreen-рендером для визуальных харнессов), выход в том же JSON.

### 4.2 Список харнессов v1 (цель: 120 к концу этапа 4, 300+ к v1)

| Группа | Примеры (имя = папка сценария) | Что сравниваем | Допуск |
|---|---|---|---|
| `fmt_*` (40) | `fmt_vpk_index`, `fmt_bsp_lumps_gm_construct`, `fmt_vtf_dxt5_mips`, `fmt_mdl_bones_combine_soldier`, `fmt_phy_hulls_crate`, `fmt_kv_surfaceprops` | наш парсер против полей, которые GMod отдаёт через Lua (`Entity:GetBoneCount`, `GetModelBounds`, `PhysObj:GetMesh`, `util.GetSurfaceData`), и против эталонных дампов VDC | точное совпадение |
| `move_*` (30) | `move_walk_flat`, `move_run_stop_friction`, `move_jump_single`, `move_bhop_10`, `move_airstrafe_circle`, `move_stairs_up_construct`, `move_crouch_jump_vent`, `move_ladder_construct_tower`, `move_water_pool`, `move_noclip`, `move_fall_damage_580`, `move_on_moving_prop` | позиция/скорость/флаги на каждом тике | 1e-3 u плоско; 0.05 u ступени/дисплейсменты; флаги точно |
| `phys_*` (40) | `phys_drop_crate_1m`, `phys_bounce_barrel_wall`, `phys_slide_crate_concrete`, `phys_stack_10_crates_settle`, `phys_ragdoll_stairs`, `phys_weld_two_props_swing`, `phys_rope_pendulum`, `phys_axis_wheel_spin`, `phys_elastic_bounce`, `phys_thruster_force`, `phys_balloon_lift`, `phys_hoverball`, `phys_jeep_accel_0_max`, `phys_explosion_push_frag` | траектории, время успокоения, финальные позы; для хаотичных сценариев - агрегаты (время, дистанция, энергия) | траектории 1-2% дистанции первые 0.5 с; успокоение +-0.3 с; финальная поза стопки: все ящики стоят/упали одинаково |
| `physgun_*` (12) | `physgun_grab_hold_still`, `physgun_drag_circle`, `physgun_scroll_distance`, `physgun_rotate_shift`, `physgun_freeze_unfreeze`, `physgun_throw_release`, `physgun_range_4096` | траектория пропа при заданном движении мыши | 2 u позиция, 2 градуса углы после фильтра |
| `tool_*` (30) | по одному на инструмент: `tool_weld_params`, `tool_rope_length_material`, `tool_remover`, `tool_dupe_roundtrip`, `tool_color_material`, `tool_light_lamp_props`, `tool_undo_stack` | созданные энтити, их свойства, констрейнты (через `constraint.GetTable`), лимиты, сообщения HUD | точно по свойствам |
| `spawn_*` (10) | `spawn_prop_placement`, `spawn_prop_limit_hit`, `spawn_npc`, `spawn_vehicle`, `spawnlist_tree`, `spawnicon_crate_render` | позиция/углы спавна, список спавнлиста, иконка (SSIM) | 0.5 u; список точно; SSIM >= 0.95 |
| `weapon_*` (30) | `weapon_pistol_fire_rate_spread`, `weapon_shotgun_pellets`, `weapon_smg1_recoil`, `weapon_crossbow_bolt_arc`, `weapon_frag_timer`, `weapon_rpg_guide`, `weapon_physcannon_punt_force`, `weapon_damage_table` | интервалы выстрелов, разброс (статистически, 1 000 выстрелов), урон по хитбоксам, боезапас | интервалы 1 тик; разброс: распределение (KS-тест p>0.05); урон точно |
| `render_*` (30) | `render_construct_spawn_view`, `render_flatgrass_sky`, `render_crate_vertexlit`, `render_water_construct_pool`, `render_glass_refract`, `render_flashlight_wall`, `render_hud_health_ammo`, `render_spawnmenu_open`, `render_pp_bloom_default` | скриншот GMod (через пробник `render.Capture` в фиксированной камере) против offscreen-рендера gmod-rust | SSIM >= 0.92 (без TAA/AA), отдельный отчёт по разнице |
| `audio_*` (10) | `audio_soundscript_params`, `audio_attenuation_curve`, `audio_footstep_interval`, `audio_soundscape_switch` | параметры/тайминги событий звука (не сэмплы) | тайминги 1 тик, параметры точно |
| `npc_*` (20) | `npc_zombie_reaction_time`, `npc_combine_accuracy_500u`, `npc_citizen_follow`, `npc_antlion_jump`, `npc_death_ragdoll` | агрегаты поведения | 20% по агрегатам в v1 |
| `net_*` (10, против себя, не против GMod) | `net_prediction_rollback_lag_150`, `net_interp_prop_20hz`, `net_64_players_bandwidth` | корректность и бюджеты нашего протокола | внутренние цели (раздел 5) |

### 4.3 Статус-таблица

Как у Horizon: `docs/status.md` генерируется из `harnesses.toml` + последнего прогона: по подсистемам оценка 0 / 0.25 (начато) / 0.5 (играбельно) / 0.75 (харнессы зелёные с допусками) / 1.0 (харнессы зелёные, допуски ужаты до целевых). Это публичная страница проекта.

### 4.4 Что мы заведомо не повторяем (осознанные расхождения, список ведётся в `docs/divergences.md`)

- Баги Source: пролёт тонких объектов, "prop surfing" через мир, краш-эксплойты.
- Сетевой протокол и его лимиты (2 048 сетевых энтити, `sv_maxrate`).
- HTML-меню и HTML-панели.
- Однопоточные ограничения (один аддон тормозит сервер).
- GLua и его API.

---

## 5. Сеть (`gm-net`)

Своя, без претензии на совместимость. Берём lightyear 0.31 (prediction/rollback, snapshot interpolation, input buffering, replicon-репликация) поверх `aeronet` (UDP/WebTransport/Steam sockets) или свой тонкий слой на `quinn` (QUIC: шифрование, потоки, NAT-friendly). Решение по результату прототипа этапа 5 (lightyear тянет Bevy ECS; если ECS у нас `hecs`, пишем своё поверх `quinn` по той же схеме).

| Параметр | Значение |
|---|---|
| Тик сервера | 66.67 Гц |
| Частота снапшотов клиенту | 33 Гц по умолчанию (GMod 20, `cl_updaterate`), до 66 |
| Частота команд от клиента | 66 Гц (каждый тик), пакет содержит 8 последних usercmd |
| Интерполяция | 2 снапшота (60 мс при 33 Гц; GMod `cl_interp` 0.1 при 20 Гц) |
| Предикция | движение игрока, оружие (стрельба, перезарядка, viewmodel), звуки игрока; пропы и NPC интерполируются |
| Lag compensation | сервер хранит 1 с истории хитбоксов (66 кадров), трассы выстрелов откатываются к `tick - rtt/2 - interp` как в Source |
| Репликация | интерес по PVS листьев + радиус 4 096 u + приоритет (игроки > удерживаемые пропы > движущиеся > спящие); дельта-снапшоты против последнего подтверждённого |
| Бюджет | <= 25 кБ/с на клиента при 64 игроках и 2 000 пропах (GMod `rate` 80 000 байт/с по умолчанию, но реально меньше); пик при спавне 500 пропов - сжатие zstd для снапшота > 1 200 байт |
| Голос | Opus 24 кГц 32 кбит/с, отдельный ненадёжный канал |
| Аутентификация | Steam ticket через Steamworks (нужен свой AppID на Steam, ~100 $) **или** своя учётка gmod-rust (OpenID Steam для привязки). Для v1: своя учётка + Steam OpenID, без AppID |
| Серверный браузер | свой мастер-сервер (HTTP + heartbeats каждые 30 с), фильтры как в GMod: карта, геймод, игроки, пинг, пароль |
| Файлы | клиент не скачивает контент с сервера (у всех он один - из GMod). Аддоны (Luau/WASM) скачиваются из реестра по хэшу манифеста сервера, лимит 50 МБ на сервер по умолчанию |

Макс. игроков: 128 (как GMod), целевой стресс-тест 64.

---

## 6. Аддоны: Luau-акторы + WASM

### 6.1 Модель исполнения

Три реалма, как в GMod, но с другими границами: `server`, `client`, `shared`(менюшный реалм не нужен, меню - лаунчер).

Аддон = пакет `.gmr` (zip): `manifest.toml` + `src/**.luau` + `wasm/*.wasm` + `assets/` (только собственные ассеты аддона, не GMod-контент). Манифест: `id`, `version` (semver), `realms`, `capabilities`, `actors`, `depends`.

Параллельность по модели Roblox Parallel Luau, но строже:

1. Каждый **актор** - отдельный Luau VM (mlua `Lua::new()` с feature `luau`), свой `_G`, свой GC, свой бюджет. Аддон объявляет акторов в манифесте (минимум один на реалм). Крупный аддон (DarkRP-подобный) делит себя на акторов по подсистемам: `economy`, `jobs`, `doors`, `hud`.
2. Тик делится на фазы:
   - **Параллельная фаза** (все акторы одновременно, пул потоков): акторы получают неизменяемый снапшот мира (`world:get(entity)` читает из снапшота, O(1)), считают, вызывают `world:queue(...)` для записи и `net:send(...)`. Любая попытка синхронной записи в мир из параллельной фазы = ошибка типов на этапе проверки (`--!strict` + наши типы `ReadonlyWorld`).
   - **Серийная фаза**: очереди применяются по порядку `priority` аддона, затем по id актора, затем по времени постановки. Хуки с возвращаемым значением (`CanPlayerSpawnProp`, `PlayerShouldTakeDamage`) исполняются в серийной фазе по цепочке приоритета, как `hook.Run` в GMod, но с явным порядком (нет "кто последний добавил").
3. Бюджеты: актор получает 2 мс CPU на тик по умолчанию (настраивается сервером до 10 мс); при превышении 5 тиков подряд актор получает предупреждение в лог и троттлится до 1 вызова в 2 тика; при исключении - актор перезапускается, остальные аддоны не затронуты. Это ответ на "один кривой аддон лагает сервер" из GMod.
4. Память: лимит на актор 256 МБ по умолчанию (mlua `set_memory_limit`).
5. Хот-релоад: `gmr reload <addon>` пересоздаёт акторов аддона с сохранением `state` (сериализуемая таблица, которую аддон сам объявляет), как Roblox/GMod `lua_refresh`, но безопасно.

### 6.2 API (v1: ~450 функций, против ~5 400 в GLua)

Пространства имён (типизированные `.d.luau` поставляются с SDK, автодополнение в VS Code через luau-lsp):

| Namespace | Содержимое (примерно) | Кол-во |
|---|---|---|
| `world` | get/queue, spawn/remove, find (by class, by name, in sphere/box, by trace), parent, I/O (`fire_input`) | 40 |
| `Entity` (userdata, read view + write-команды) | pos/angles/velocity, model/skin/bodygroup/sequence, color/material/rendermode, health, owner, phys (mass, frozen, motion, apply_force/impulse, material), collision group, bounds, hitboxes, attachments, bones | 110 |
| `Player: Entity` | input (buttons, eye angles), noclip, speeds, team, armor, weapons/ammo, chat/say, notify, undo, limits, permissions, steamid/account | 70 |
| `phys` | trace (line/hull/sphere), constraints create/remove/get, materials, collide callbacks | 35 |
| `events` | `on(name, fn, priority)`, типизированные события (см. ниже) | 10 |
| `net` | `define(message, schema)`, `send(to, msg)`, `broadcast`, `on`, reliable/unreliable, лимит 64 кБ/с на аддон-игрока | 12 |
| `timer`/`task` | `delay`, `every`, `cancel`, `task.wait/spawn/defer` | 10 |
| `tool` | регистрация инструмента: `left/right/reload`, `ghost`, `panel(schema)`, convars | 15 |
| `spawnmenu` | категории, спавнлисты, вкладки, иконки | 15 |
| `ui` (client) | панели (тот же набор, что в 3.8), `hud.register(layer, draw)`, `draw.*` (rect, text, texture, poly, 3D2D), шрифты | 90 |
| `render` (client) | камеры/вьюпорты, модели в UI, рендер-таргеты, материалы (создание из своих ассетов), пост-эффекты | 30 |
| `sound` | play (world/entity/client), soundscripts, стоп, параметры, `sound.create(ogg)` | 12 |
| `storage` | KV на аддон (sqlite за кулисами), файлы только в своей папке, квота 100 МБ | 8 |
| `cvar`/`cmd` | конфиги с правами, команды с правами | 10 |
| `game` | карта, тик, время, лимиты, клинап, геймод-инфо | 15 |
| `wasm` | загрузка компонентов, вызовы с типами из WIT | 6 |

События (вместо 267 GLua-хуков в v1 ~90): `player.join/leave/spawn/death/damage(cancelable)/chat(cancelable)/use/noclip`, `entity.spawned/removed/collide/take_damage(cancelable)`, `prop.spawn_request(cancelable)`, `tool.before/after`, `physgun.pickup(cancelable)/drop/freeze`, `tick.pre/post`, `round` для геймодов, `net.*`, `ui.*` (client), `render.frame/hud/scene` (client).

Геймоды: аддон с типом `gamemode` (sandbox - тоже аддон, написанный на Luau поверх API: это главный dogfooding всего API; `gm-game` держит только то, что требует Rust-скорости: физган-контроллер, трассы, лимиты).

### 6.3 WASM-компоненты

Для того, что Luau не потянет (процедурная генерация, свои физические контроллеры, компрессия, ML): wasmtime component model, WIT-мир `gmodrust:addon/host@0.1` с теми же namespace, что Luau API (общая кодогенерация из одного описания API в `.d.luau` и `.wit`). Языки: Rust (первый класс), C/C++, C# через componentize-dotnet (помечаем экспериментальным, тулчейн prerelease). Ограничения: `Store` на компонент, память до 512 МБ, fuel-лимит на вызов, capabilities те же, что у Luau-актора. Горячие хост-вызовы (трассы) - batched API (`trace_many`), чтобы 30-90 нс на вызов не превращались в проблему.

### 6.4 Безопасность и права

Capabilities в манифесте, показываются при установке (как у Horizon): `world-read`, `world-write`, `players-admin` (кики/баны/права), `net`, `storage`, `ui-hud`, `ui-fullscreen`, `sound`, `cvar-write`, `shell` (нет такого: аддоны никогда не получают ФС/процессы/сокеты, кроме `net` к своему серверу). Клиентские аддоны устанавливает игрок; серверные аддоны клиент получает только их клиентскую часть, с теми же capability-проверками, без доступа за пределы песочницы.

---

## 7. Лаунчер и экосистема

### 7.1 Лаунчер (Rust, egui/iced, минималистичный)

- Находит Steam (`libraryfolders.vdf`), GMod (app 4000), проверяет файлы. Если GMod нет - ссылка на Steam, запуск невозможен.
- Собирает/обновляет кэш ресурсов (раздел 1.2) с прогресс-баром; инкрементально при обновлении GMod.
- Обновления gmod-rust: релизы с GitHub Actions, `SHA256SUMS` + подпись (minisign), лаунчер проверяет перед установкой, канал `stable`/`beta`.
- Серверный браузер, избранное, история, прямое подключение, пароль.
- Аддоны: вкладка реестра (поиск, установка клиентских аддонов, права), локальная папка `addons/`.
- Профили настроек, выбор видеокарты, лог/краш-репорты (opt-in, как у Horizon: по умолчанию ничего не собираем).
- Запуск клиента / локального сервера (single player = локальный сервер в том же процессе на отдельном потоке, как Horizon после #1514 и как GMod).

### 7.2 Реестр аддонов

- Публикация через `gmr publish` (CLI из SDK): подпись ключом автора, semver, зависимости, манифест capabilities.
- Сервер реестра: Rust (axum) + Postgres + S3-совместимое хранилище; веб-страница аддона с правами и исходниками (аддоны открыты по умолчанию: `.gmr` содержит исходники Luau; WASM - с опциональной ссылкой на репозиторий).
- Модерация: автоматическая проверка (манифест, размер, запрещённые capabilities без обоснования, отсутствие GMod-контента внутри `.gmr` - тот же `check_public_tree`), ручная для "verified".
- Сервера объявляют список аддонов хэшами; клиент докачивает недостающее из реестра (не с сервера).

### 7.3 SDK

- `cargo install gmr-sdk`: `gmr new addon|gamemode|tool`, `gmr dev` (хот-релоад против локального сервера), `gmr test` (харнессы для аддонов: сценарий -> ожидаемое состояние), `gmr publish`.
- Типы `.d.luau`, luau-lsp конфиг, шаблоны, документация API генерируется из одного источника (тот же, что `.wit`).

---

## 8. Этапы и критерии выхода

Оценки в "агентных неделях" условные: это объём, не календарь. Параллельно идёт не более 3 треков.

| Этап | Содержимое | Критерий выхода (измеримый) |
|---|---|---|
| **0. Фундамент** | workspace, CI `verify`, `check_public_tree`, машина CI с GMod, пробник GLua + runner + compare, первые 10 `fmt_*` харнессов, спецификации `docs/spec/` для движения и физики | 10 харнессов зелёные; PR без зелёного `verify` не мержится |
| **1. Контент** | VPK/BSP/VTF/VMT/MDL/PHY/KeyValues парсеры, пайплайн кэша, лаунчер-минимум (найти GMod, собрать кэш), рендер BSP с лайтмапами, статик-пропы, скайбокс, модели VertexLit, свободная камера | gm_construct и gm_flatgrass рендерятся, `render_*` SSIM >= 0.85 на 5 камерах, 40 `fmt_*` зелёные, кэш полной GMod собирается < 15 мин |
| **2. Движение** | `gm-move` полностью, хулл-трассы по BSP, лестницы, вода, noclip, HUD здоровья, локальный сервер в процессе | 30 `move_*` зелёные в целевых допусках |
| **3. Физика и физган** | Jolt, `.phy`, surfaceprops, констрейнты базовые (weld/rope/axis/ballsocket/nocollide), спавн пропов из спавн-меню (UI), физган, undo, звуки ударов, рэгдоллы | 40 `phys_*` + 12 `physgun_*` зелёные; 5 000 активных тел <= 3 мс/тик на 8 ядрах |
| **4. Sandbox** | тулган и все base-инструменты, контекст-меню, лимиты, клинап, дубликатор, лампы/свет/thruster/balloon/wheel/hoverball/button/dynamite/emitter, пост-процессы, спавн-иконки | 30 `tool_*` + 10 `spawn_*` зелёные; одиночка играбельна "как GMod sandbox без оружия" |
| **5. Сеть** | протокол, предикция, интерполяция, lag comp, мастер-сервер, dedicated server, голос, учётки | 64 игрока на одном сервере с 2 000 пропов: тик <= 6 мс, трафик <= 25 кБ/с на клиента, `net_*` зелёные |
| **6. Аддоны** | Luau-акторы, API v1 (450 функций), WASM, SDK, хот-релоад, sandbox переписан как Luau-геймод поверх API | sandbox-геймод на Luau проходит те же `tool_*`/`spawn_*` харнессы, что Rust-версия; демо-аддон на 3 актора параллельно загружает 4 ядра без потери тика |
| **7. Оружие** | все HL2 + GMod оружия, боезапас, урон, хитбоксы, viewmodel-анимации, decals, частицы PCF (подмножество) | 30 `weapon_*` зелёные |
| **8. Экосистема** | лаунчер полный, реестр, подписи релизов, статус-страница, документация, публичная бета | 100 внешних игроков на публичных серверах в течение недели без краша сервера; статус-страница публикуется автоматически |
| **9. NPC и транспорт** | боевой контур NPC HL2, navmesh/.ain, транспорт Jeep/Airboat | 20 `npc_*` в допуске 20%; `phys_jeep_*` зелёные |
| **10+. Долгий хвост** | полный AI HL2, сцены/лицевая анимация, остальные операторы частиц, Steam Audio, VR? | по харнессам |

Риски и что с ними делаем:

- **Юридический** (Valve/Facepunch). Снижаем: ни байта контента, clean-room, нейтральное имя, некоммерческий статус, открытая переписка (Facepunch в целом лояльны к сообществу; Valve терпят Xash3D/OpenMW-подобные проекты годами). Остаточный риск есть, его принимаем осознанно.
- **Паритет физики** (IVP/VPhysics vs Jolt). Снижаем харнессами и подгонкой параметров; принимаем, что хаотичные сценарии совпадают статистически, не траекторно.
- **NPC AI** - самый большой и плохо документированный объём. Выносим за v1.
- **Люди**: проект одного заказчика + агенты. Horizon показывает, что это работает при жёстком машинном контроле качества, поэтому харнессы и CI идут первыми, до "красивого".

---

## 9. Открытые вопросы к тебе

1. Имя продукта на публику: "gmod-rust" содержит марки Facepunch (GMod, Rust), для репозитория оставлено, для продукта решить позже.
2. Платформы v1: Windows + Linux (у GMod оба). macOS можно позже (wgpu/Metal есть, но GMod на macOS 32-битный и Steam его почти не ставит - проверить).
3. Учётки: своя регистрация + Steam OpenID (бесплатно) или сразу Steamworks AppID (100 $, даёт Steam-аутентификацию и голос через Steam).
4. Какой первый трек после фундамента: контент+рендер (видно результат) или движение+физика (чувствуется результат). Я бы шёл контент -> движение -> физика, как в таблице.
5. Публичность: репозиторий открыт с первого дня (как Horizon) или до играбельной одиночки закрыт.

---

## 10. Лаборатория реверса: откуда берётся знание о поведении

Horizon "написан по самой игре": `tools/lab.py decompile` раскладывает официальные JAR через Vineflower в гитигнорный `.local/`, агенты читают декомпилят (11 383 клиентских и 7 762 серверных класса) и пишут Rust по нему, харнессы гоняют ванильный байткод как оракул. В Git и релизах игры нет ни строки. Мы делаем то же, с поправкой на то, что Source это нативный код, а не Java.

### 10.1 Три слоя источников, по убыванию удобства

| Слой | Что покрывает | Как читаем | Доля нужного нам поведения |
|---|---|---|---|
| **Source SDK 2013** (открытый C++ game-код: `game/shared`, `game/server`, `game/client`) | движение игрока целиком (`gamemovement`), всё оружие HL2, боезапас/урон, NPC AI (схедулы, nodegraph, squads), транспорт, частицы (клиентская часть), VGUI-контролы, HUD-элементы, entity I/O, `prop_physics` логика (взаимодействие с VPhysics через интерфейс `IPhysics*`), шейдеры `stdshaders` (HLSL VertexLitGeneric/LightmappedGeneric и др.) | как спецификацию: агент-спецификатор пишет `docs/spec/*.md` своими словами и числами, агент-реализатор видит только спек | ~60% |
| **Lua-база GMod** (Facepunch/garrysmod, открыта, без лицензии) | sandbox: спавн-меню, инструменты тулгана и их параметры, контекст-меню, лимиты, undo, клинап, дубликатор, HUD sandbox, Derma и скин | как данные и спецификацию, не копируем | ~15% |
| **Реверс бинарей GMod** (то, что закрыто) | `vphysics` (shadow-контроллер физгана: скорости/дэмпинг/телепорт-порог, пороги сна, параметры констрейнтов, звуки ударов по скорости, трение/упругость как применяются к Havok), `engine` (тик-луп, интерполяция, лимиты, PVS/сеть - для харнессов тайминга), `server`/`client` Facepunch (физган, тулган-база, `gmod_*` энтити, спавн пропов, цвет игрока, `PlayerColor`-прокси), `materialsystem`/`shaderapi` (тонмаппинг, bloom, порядок прозрачности) | Ghidra headless + агенты; Linux-бинари `srcds` имеют экспортированные символы (не стрипнуты), что даёт имена функций бесплатно; результаты в гитигнорный `.local/re/` | ~25% |

Приоритет реверса: **vphysics > физган/тулган в server.dll > materialsystem**. Всё остальное закрывается SDK и Lua.

### 10.2 Инструменты и рабочий процесс

```
tools/lab/
  prepare.py      # находит Steam-копию (или SteamCMD-загрузку app 4000/4020), проверяет appmanifest, копирует bin/ в .local/gmod/<build>/
  symbols.py      # вытаскивает экспорт-символы из Linux .so (nm/readelf), строит карту "имя -> адрес" и сопоставляет с Windows-билдом по сигнатурам
  ghidra.sh       # headless-анализ: импорт, авто-анализ, экспорт декомпилята в .local/re/<build>/<module>/*.c (гитигнор)
  notes/          # ЧИТАЕМЫЕ ЛЮДЬМИ заметки по поведению: числа, формулы, автоматы состояний. Это коммитится. Без кусков декомпилята.
  check_notes.py  # запрещает в notes/ и docs/spec/ фрагменты кода из .local/re/ (n-gram сравнение с декомпилятом) и имена из утечек
```

Правила (расширяем `check_public_tree`):
1. `.local/` в `.gitignore` и в pre-commit хуке; CI падает, если в дереве появился файл с сигнатурой PE/ELF из GMod, VPK/BSP/VTF/MDL, или текст, совпадающий с декомпилятом (n-gram >= 12 токенов).
2. В `docs/spec/` и в код попадают только **поведение и числа**: "физган тянет объект к целевой точке с ограничением скорости V и углового ускорения W, телепортирует при расстоянии > D" - да; `if (dist > 32.0f) { pObj->SetPosition(...) }` из декомпилята - нет.
3. Утечки Source (2007, 2018/2020) не используются вообще, ни для чтения. Это отличает нас от RedSource и это проверяемо: в заметках не должно быть имён, которых нет ни в SDK 2013, ни в экспортированных символах.
4. Каждая заметка ссылается на харнесс, который её подтверждает. Заметка без харнесса - гипотеза.

Что уже есть у нас: в `gmod-steamaudio` мы делали ровно это для аудио-пайплайна x64-билда 25464497 (точки перехвата микшера, структура paint-дескрипторов, эпохи устройства). Этот опыт и инструменты переносятся; аудио-часть спека по сути уже написана.

### 10.3 Юридическая разница с Minecraft (честно)

- Minecraft EULA допускает модификации и де-факто терпит декомпиляцию (вся Forge/Fabric-экосистема на ней стоит). Steam Subscriber Agreement и EULA Valve запрещают реверс-инжиниринг.
- Исключения для интероперабельности: Директива ЕС 2009/24/EC ст. 6 и ГК РФ ст. 1280 п. 3 разрешают декомпиляцию для обеспечения взаимодействия с другой программой при соблюдении условий (не распространять декомпилят, не использовать для создания существенно схожей программы в выражении кода). Наша практика (поведение и числа в спеке, код своими словами, декомпилят не покидает машину) под эти условия подогнана, но это не гарантия.
- Практика сообщества: вся сцена GMod-бинарных модулей (включая наш `gmod-steamaudio`), SourceMod/Metamod, Xash3D, OpenMW живут в этой же серой зоне годами; Valve на такие проекты исторически не нападает, Facepunch тем более. Риск принимаем осознанно и снижаем тем, что в репе нет ни байта чужого.

### 10.4 Харнессы и бинари: что нужно на CI-машине

- **Dedicated server** (Steam app 4020, Linux, анонимный логин **не** работает для GMod-сервера? - проверить; у Valve большинство srcds доступны анонимно, для 4020 да): серверные харнессы (`move_*` серверная сторона, `phys_*`, `tool_*`, `spawn_*`, `weapon_*` по урону/таймингам). Linux `srcds` даёт символы для реверса `vphysics_srv.so`, `server_srv.so`, `engine_srv.so`.
- **Клиент** (app 4000, нужен владелец): визуальные харнессы (`render_*`), клиентская предикция, HUD, физган-ощущение с реальным вводом, спавн-иконки. Клиент на CI запускается под Windows или под Linux с Xvfb + софтверным Vulkan (для скриншотов годится, для FPS нет). Для реверса берём `bin/win64/*.dll` из Windows x86-64 ветки (64-битные бинари) и Linux `.so` (символы).
- Твоя лицензионная копия через SteamCMD с твоим логином: даёт оба приложения. Клиентские файлы (~15 ГБ с Workshop-подписками, ~6 ГБ без) лежат только на CI-машине, в репу не попадают.
