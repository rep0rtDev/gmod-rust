-- Shared / server libraries without an engine object behind them: pure-Lua state (team, list,
-- numpad, player_manager, duplicator registries, Tool state), thin wrappers over what the host
-- has (umsg over net, physenv over cvars), honest stubs for the rest (lists come from the manifest).
local stub = __glua_stub

team = team or {}
local teams, team_scores, team_spawns = {}, {}, {}
TEAM_CONNECTING, TEAM_UNASSIGNED, TEAM_SPECTATOR = 0, 1001, 1002
function team.SetUp(id, name, color, joinable) teams[id] = { Name = name, Color = color or Color(255, 255, 255), Joinable = joinable ~= false, Class = {} }; team_scores[id] = team_scores[id] or 0 end
function team.Valid(id) return teams[id] ~= nil end
function team.GetName(id) return teams[id] and teams[id].Name or "" end
function team.GetColor(id) return teams[id] and teams[id].Color or Color(255, 255, 255) end
function team.SetColor(id, c) if teams[id] then teams[id].Color = c end end
function team.Joinable(id) return teams[id] ~= nil and teams[id].Joinable end
function team.GetAllTeams() return teams end
function team.GetScore(id) return team_scores[id] or 0 end
function team.SetScore(id, s) team_scores[id] = s end
function team.AddScore(id, s) team_scores[id] = (team_scores[id] or 0) + s end
function team.GetClass(id) return teams[id] and teams[id].Class or {} end
function team.SetClass(id, c) if teams[id] then teams[id].Class = c end end
function team.GetSpawnPoint(id) return team_spawns[id] or {} end
function team.GetSpawnPoints(id) local out = {}; for _, cls in ipairs(team_spawns[id] or {}) do for _, e in ipairs(ents.FindByClass(cls)) do out[#out + 1] = e end end return out end
function team.SetSpawnPoint(id, cls) team_spawns[id] = istable(cls) and cls or { cls } end
function team.GetPlayers(id) local out = {}; for _, p in ipairs(player.GetAll()) do if p:Team() == id then out[#out + 1] = p end end return out end
function team.NumPlayers(id) return #team.GetPlayers(id) end
function team.TotalFrags(id) local n = 0; for _, p in ipairs(team.GetPlayers(id)) do n = n + p:Frags() end return n end
function team.TotalDeaths(id) local n = 0; for _, p in ipairs(team.GetPlayers(id)) do n = n + p:Deaths() end return n end
function team.BestAutoJoinTeam() local best, bn = TEAM_UNASSIGNED, math.huge; for id, t in pairs(teams) do if t.Joinable and id > 0 and id < 1000 then local n = team.NumPlayers(id); if n < bn then best, bn = id, n end end end return best end

list = list or {}
local lists = {}
function list.Set(name, key, value) lists[name] = lists[name] or {}; lists[name][key] = value end
function list.Add(name, value) lists[name] = lists[name] or {}; local t = lists[name]; t[#t + 1] = value; return #t end
function list.Get(name) return table.Copy(lists[name] or {}) end
function list.GetForEdit(name, dontCreate) if not lists[name] and not dontCreate then lists[name] = {} end return lists[name] end
function list.GetTable() local out = {}; for k in pairs(lists) do out[#out + 1] = k end return out end
function list.Contains(name, value) for _, v in pairs(lists[name] or {}) do if v == value then return true end end return false end
function list.HasEntry(name, key) return lists[name] ~= nil and lists[name][key] ~= nil end
function list.GetEntry(name, key) return lists[name] and lists[name][key] end
function list.RemoveEntry(name, key) if lists[name] then lists[name][key] = nil end end

numpad = numpad or {}
local numpad_fns, numpad_binds, numpad_id = {}, {}, 0
function numpad.Register(name, fn) numpad_fns[name] = fn end
local function numpad_bind(ply, key, name, kind, ...) numpad_id = numpad_id + 1; local uid = IsValid(ply) and ply:UserID() or 0; numpad_binds[uid] = numpad_binds[uid] or {}; numpad_binds[uid][key] = numpad_binds[uid][key] or {}; numpad_binds[uid][key][numpad_id] = { kind = kind, name = name, args = { ... } }; return numpad_id end
function numpad.OnDown(ply, key, name, ...) return numpad_bind(ply, key, name, "down", ...) end
function numpad.OnUp(ply, key, name, ...) return numpad_bind(ply, key, name, "up", ...) end
function numpad.Remove(id) for _, keys in pairs(numpad_binds) do for _, t in pairs(keys) do t[id] = nil end end end
local function numpad_fire(ply, key, kind) local uid = IsValid(ply) and ply:UserID() or 0; local t = numpad_binds[uid] and numpad_binds[uid][key]; if not t then return end for _, b in pairs(t) do if b.kind == kind and numpad_fns[b.name] then numpad_fns[b.name](ply, unpack(b.args)) end end end
function numpad.Activate(ply, key, isButton) numpad_fire(ply, key, "down") end
function numpad.Deactivate(ply, key, isButton) numpad_fire(ply, key, "up") end
function numpad.Toggle(ply, key) local uid = IsValid(ply) and ply:UserID() or 0; local st = numpad_binds["toggle" .. uid] or {}; numpad_binds["toggle" .. uid] = st; if st[key] then st[key] = nil; numpad_fire(ply, key, "up") else st[key] = true; numpad_fire(ply, key, "down") end end
numpad.FromButton = stub("numpad.FromButton")

player_manager = player_manager or {}
local pm_models, pm_hands, pm_classes, pm_player_class = {}, {}, {}, {}
function player_manager.AddValidModel(name, model) pm_models[name] = model end
function player_manager.RemoveValidModel(name) pm_models[name] = nil end
function player_manager.AllValidModels() return pm_models end
function player_manager.GetAllPlayerModels() local out = {}; for _, m in pairs(pm_models) do out[#out + 1] = m end return out end
function player_manager.TranslatePlayerModel(name) return pm_models[name] or pm_models[string.lower(name or "")] or "models/player/kleiner.mdl" end
function player_manager.TranslateToPlayerModelName(model) for n, m in pairs(pm_models) do if m == model then return n end end return "kleiner" end
function player_manager.AddValidHands(name, model, skin, body, matchBodySkin) pm_hands[name] = { model = model, skin = skin or 0, body = body or "0000000", matchBodySkin = matchBodySkin == true } end
function player_manager.TranslatePlayerHands(name) return pm_hands[name] or { model = "models/weapons/c_arms_citizen.mdl", skin = 0, body = "0000000" } end
function player_manager.RegisterClass(name, tbl, base) tbl.Base = base; tbl.ClassName = name; pm_classes[name] = tbl end
function player_manager.GetPlayerClasses() return pm_classes end
function player_manager.SetPlayerClass(ply, name) pm_player_class[ply:UserID()] = name; ply:SetNWString("__pm_class", name); player_manager.RunClass(ply, "ClassChanged") end
function player_manager.GetPlayerClass(ply) return pm_player_class[ply:UserID()] end
function player_manager.ClearPlayerClass(ply) pm_player_class[ply:UserID()] = nil end
function player_manager.GetPlayerClassTable(ply) local n = pm_player_class[ply:UserID()]; return n and pm_classes[n] or nil end
function player_manager.RunClass(ply, fn, ...) local c = player_manager.GetPlayerClassTable(ply); while c do if c[fn] then return c[fn](c, ply, ...) end c = c.Base and pm_classes[c.Base] or nil end end
function player_manager.OnPlayerSpawn(ply, transition) player_manager.RunClass(ply, "Spawn"); if not transition then player_manager.RunClass(ply, "Loadout") end end

system = system or {}
function system.IsLinux() return true end
function system.IsWindows() return false end
function system.IsOSX() return false end
function system.HasFocus() return true end
function system.UpTime() return RealTime() end
function system.AppTime() return RealTime() end
function system.SteamTime() return os.time() end
function system.GetCountry() return "RU" end
function system.BatteryPower() return 255 end

umsg = umsg or {}
local umsg_filter
function umsg.Start(name, filter) umsg_filter = filter; net.Start(name) end
function umsg.End() if umsg_filter == nil then net.Broadcast() elseif IsEntity(umsg_filter) then net.Send(umsg_filter) else net.Send(umsg_filter) end umsg_filter = nil end
function umsg.Angle(a) net.WriteAngle(a) end
function umsg.Bool(b) net.WriteBool(b) end
function umsg.Char(c) net.WriteInt(c, 8) end
function umsg.Entity(e) net.WriteEntity(e) end
function umsg.Float(f) net.WriteFloat(f) end
function umsg.Long(l) net.WriteInt(l, 32) end
function umsg.Short(s) net.WriteInt(s, 16) end
function umsg.String(s) net.WriteString(s) end
function umsg.Vector(v) net.WriteVector(v) end
function umsg.VectorNormal(v) net.WriteNormal(v) end
function umsg.PoolString(s) end
function SendUserMessage(name, filter, ...) umsg.Start(name, filter); for _, v in ipairs({ ... }) do if isbool(v) then umsg.Bool(v) elseif isnumber(v) then umsg.Float(v) elseif isstring(v) then umsg.String(v) elseif isvector(v) then umsg.Vector(v) elseif isangle(v) then umsg.Angle(v) elseif IsEntity(v) then umsg.Entity(v) end end umsg.End() end

physenv = physenv or {}
function physenv.GetGravity() return Vector(0, 0, -(GetConVar("sv_gravity") and GetConVar("sv_gravity"):GetFloat() or 600)) end
function physenv.GetTimeScale() return 1 end
function physenv.GetPhysicsPaused() return false end
function physenv.GetAirDensity() return 2 end
function physenv.GetLastSimulationTime() return engine.TickInterval() end
for _, n in ipairs({ "AddSurfaceData", "GetPerformanceSettings", "SetAirDensity", "SetGravity", "SetPerformanceSettings", "SetPhysicsPaused", "SetTimeScale" }) do if physenv[n] == nil then physenv[n] = stub("physenv." .. n) end end

function game.GetAmmoDamageType(id) local d = game.GetAmmoData and game.GetAmmoData(id); return d and d.dmgtype or DMG_BULLET end
function game.GetAmmoForce(id) local d = game.GetAmmoData and game.GetAmmoData(id); return d and d.force or 1000 end
function game.GetAmmoNPCDamage(id) local d = game.GetAmmoData and game.GetAmmoData(id); return d and d.npcdmg or 0 end
function game.GetAmmoPlayerDamage(id) local d = game.GetAmmoData and game.GetAmmoData(id); return d and d.plydmg or 0 end
for _, n in ipairs({ "Get3DSkyboxInfo", "GetMapChangeCount", "GetWindSpeed", "MapLoadType" }) do if game[n] == nil then game[n] = stub("game." .. n) end end

function util.IntersectRayWithSphere(o, d, c, r)
    local m = o - c; local b = m:Dot(d); local cc = m:Dot(m) - r * r
    if cc > 0 and b > 0 then return nil end
    local disc = b * b - cc; if disc < 0 then return nil end
    local s = math.sqrt(disc); local len = d:Length()
    return math.max(0, (-b - s) / len), (-b + s) / len
end
function util.IntersectRayWithTriangle(o, d, a, b, c)
    local e1, e2 = b - a, c - a; local p = d:Cross(e2); local det = e1:Dot(p)
    if math.abs(det) < 1e-8 then return nil end
    local inv = 1 / det; local t = o - a; local u = t:Dot(p) * inv; if u < 0 or u > 1 then return nil end
    local q = t:Cross(e1); local v = d:Dot(q) * inv; if v < 0 or u + v > 1 then return nil end
    local dist = e2:Dot(q) * inv; if dist < 0 then return nil end
    return dist
end
function util.IsBoxIntersectingBox(a1, a2, b1, b2) return a1.x <= b2.x and a2.x >= b1.x and a1.y <= b2.y and a2.y >= b1.y and a1.z <= b2.z and a2.z >= b1.z end
function util.LocalToWorld(ent, lpos, bone) return ent:LocalToWorld(lpos) end
function util.SharedRandom(name, lo, hi, add) local seed = util.CRC(name .. tostring(math.floor(CurTime() * 100)) .. tostring(add or 0)); return lo + (tonumber(seed) % 10000) / 10000 * (hi - lo) end
function util.KeyValuesToTablePreserveOrder(kv, usesEscapes, preserveKeyCase) local t = util.KeyValuesToTable(kv, usesEscapes, preserveKeyCase); local out = {}; for k, v in pairs(t) do out[#out + 1] = { Key = k, Value = v } end return out end
function util.TraceEntityHull(data, ent) local mins, maxs = ent:OBBMins(), ent:OBBMaxs(); data.mins, data.maxs = mins, maxs; return util.TraceHull(data) end
function util.GetActivityIDByName(name) local v = _G[tostring(name)]; if type(v) == "number" and tostring(name):sub(1, 4) == "ACT_" then return v end return -1 end
local ACT_NAMES = {}
for k, v in pairs(_G) do if type(k) == "string" and k:sub(1, 4) == "ACT_" and type(v) == "number" then ACT_NAMES[v] = k end end
function util.GetActivityNameByID(id) return ACT_NAMES[tonumber(id) or -2] or "" end
for _, n in ipairs({ "GetAnimEventIDByName", "GetAnimEventNameByID", "GetUserGroups", "SHA1", }) do if util[n] == nil then util[n] = stub("util." .. n) end end

function VectorRand(lo, hi) lo, hi = lo or -1, hi or 1; return Vector(math.Rand(lo, hi), math.Rand(lo, hi), math.Rand(lo, hi)) end
function AngleRand(lo, hi) lo, hi = lo or -90, hi or 90; return Angle(math.Rand(lo, hi), math.Rand(math.max(lo, -180), math.min(hi, 180)), math.Rand(lo, hi)) end
function LocalToWorld(lpos, lang, pos, ang) local m = Matrix(); m:SetTranslation(pos); m:SetAngles(ang); local p = m * lpos; local a = Angle(lang.p, lang.y, lang.r); local out = ang + a; return p, out end
function WorldToLocal(wpos, wang, pos, ang) local m = Matrix(); m:SetTranslation(pos); m:SetAngles(ang); m:InvertTR(); return m * wpos, wang - ang end
function OrderVectors(a, b) local mn = Vector(math.min(a.x, b.x), math.min(a.y, b.y), math.min(a.z, b.z)); local mx = Vector(math.max(a.x, b.x), math.max(a.y, b.y), math.max(a.z, b.z)); a.x, a.y, a.z, b.x, b.y, b.z = mn.x, mn.y, mn.z, mx.x, mx.y, mx.z end
function HexToColor(hex) hex = hex:gsub("#", ""); return Color(tonumber(hex:sub(1, 2), 16) or 0, tonumber(hex:sub(3, 4), 16) or 0, tonumber(hex:sub(5, 6), 16) or 0, tonumber(hex:sub(7, 8), 16) or 255) end
function HWBToColor(h, w, b) return HSVToColor(h, 1 - w / math.max(1e-6, 1 - b), 1 - b) end
function SafeRemoveEntity(e) if IsValid(e) and not (e.IsPlayer and e:IsPlayer()) then e:Remove() end end
function SafeRemoveEntityDelayed(e, delay) timer.Simple(delay, function() SafeRemoveEntity(e) end) end
function IsTableOfEntitiesValid(t) if not istable(t) or next(t) == nil then return false end for _, e in pairs(t) do if not IsValid(e) then return false end end return true end
function Error(...) error(table.concat({ ... }, ""), 2) end
function ServerLog(s) print(s) end
function FrameNumber() return engine.TickCount() end
function TimedSin(freq, origin, max, offset) return math.sin(freq * CurTime() * 2 * math.pi + offset) * (max - origin) / 2 + (max + origin) / 2 end
function TimedCos(freq, origin, max, offset) return math.cos(freq * CurTime() * 2 * math.pi + offset) * (max - origin) / 2 + (max + origin) / 2 end
function STNDRD(n) if n % 100 >= 11 and n % 100 <= 13 then return "th" end return ({ "st", "nd", "rd" })[n % 10] or "th" end
function SQLStr(s, noQuotes) local e = tostring(s):gsub("'", "''"); return noQuotes and e or ("'" .. e .. "'") end
function Model(path) return path end
function Sound(path) return path end
function RegisterMetaTable(name, tbl) __glua_classes[name] = tbl end
function IsEnemyEntityName(c) return ({ npc_zombie = true, npc_fastzombie = true, npc_poisonzombie = true, npc_headcrab = true, npc_headcrab_fast = true, npc_headcrab_poison = true, npc_antlion = true, npc_antlionguard = true, npc_combine_s = true, npc_metropolice = true, npc_manhack = true, npc_hunter = true, npc_strider = true, npc_barnacle = true, npc_zombine = true })[c] == true end
function IsFriendEntityName(c) return ({ npc_citizen = true, npc_alyx = true, npc_barney = true, npc_kleiner = true, npc_eli = true, npc_mossman = true, npc_monk = true, npc_vortigaunt = true, npc_dog = true, npc_magnusson = true })[c] == true end
function IsUselessModel(m) m = string.lower(m or ""); return m:find("_gesture") ~= nil or m:find("_anim") ~= nil or m:find("_gestures") ~= nil or m:find("_postures") ~= nil or m:find("_shared") ~= nil or m:find("_ss") ~= nil or m:find("_arms") ~= nil or m:find("_animations") ~= nil end
UTIL_IsUselessModel = IsUselessModel
BRANCH = "x86-64"
-- Chunks from strings, compiled in this VM's sandbox (the game's RunString / CompileString;
-- CompileFile and include need files the actor does not have).
function CompileString(code, identifier, handleError)
    local fn, err = __compile(tostring(code), tostring(identifier or "CompileString"))
    if fn then return fn end
    if handleError == false then return err end
    error(err, 2)
end
function RunString(code, identifier, handleError)
    local fn, err = __compile(tostring(code), tostring(identifier or "RunString"))
    if fn then
        local ok, e = pcall(fn)
        if ok then return nil end
        err = tostring(e)
    end
    if handleError ~= false then ErrorNoHalt(err .. "\n") end
    return err
end
RunStringEx = RunString
-- Mounted content (provider labels from the host) and sound lengths from its WAV headers.
function IsMounted(name)
    name = string.lower(tostring(name or ""))
    for _, l in ipairs(__mounted()) do if string.find(string.lower(l), name, 1, true) then return true end end
    return false
end
function SoundDuration(name) return __sound_duration(tostring(name or "")) or 0 end
for _, n in ipairs({ "AddConsoleCommand", "AddOriginToPVS", "Add_NPC_Class", "BroadcastLua", "BuildNetworkedVarsTable", "CompileFile", "CreatePhysCollideBox", "CreatePhysCollidesFromModel", "CreateSound", "DEFINE_BASECLASS", "DTVar_ReceiveProxyGL", "DebugInfo", "EmitSentence", "GetPredictionPlayer", "HTTP", "IsFirstTimePredicted", "Material", "Particle", "ParticleEffect", "ParticleEffectAttach", "PrecacheParticleSystem", "PrecacheScene", "PrecacheSentenceFile", "PrecacheSentenceGroup", "SentenceDuration", "SetPhysConstraintSystem", "SuppressHostEvents", "TauntCamera" }) do if _G[n] == nil then _G[n] = stub("Global." .. n) end end


-- Tool: the tool gun's per-mode state object (ghost entities need the client realm).
local TOOL = {}
TOOL.__index = TOOL
-- scripted stools: a TOOL instance registers under its Mode the moment the addon assigns it (our
-- addons are manifest actors, there is no stools/<mode>.lua loader, so `TOOL.Mode = "x"` is
-- required); the server's click on a mode no Rust stool owns reaches it through the host `__tool`
local glua_tools = {}
local tool_inst_meta = { __index = TOOL, __newindex = function(t, k, v) rawset(t, k, v) if k == "Mode" and type(v) == "string" then glua_tools[v] = t end end }
function TOOL:Create() return setmetatable({ Mode = nil, SWEP = nil, Owner = nil, ClientConVar = {}, ServerConVar = {}, Objects = {}, Stage = 0, Operation = 0, Message = "start", LastMessage = 0, AllowedCVar = 0 }, tool_inst_meta) end
function TOOL:GetMode() return self.Mode end
function TOOL:GetOwner() return self.Owner or (self.SWEP and self.SWEP.Owner) or NULL end
function TOOL:GetSWEP() return self.SWEP end
function TOOL:GetWeapon() return self.SWEP end
function TOOL:GetStage() return self.Stage end
function TOOL:SetStage(s) self.Stage = s end
function TOOL:GetOperation() return self.Operation end
function TOOL:SetOperation(o) self.Operation = o end
function TOOL:GetHelpText() return "#tool." .. tostring(self.Mode) .. "." .. tostring(self.Stage) end
function TOOL:Allowed() local cv = GetConVar("toolmode_allow_" .. tostring(self.Mode)); return cv == nil or cv:GetBool() end
function TOOL:NumObjects() return #self.Objects end
function TOOL:ClearObjects() self.Objects = {} end
function TOOL:SetObject(i, ent, pos, phys, bone, norm) self.Objects[i] = { Ent = ent, Pos = pos, Phys = phys, Bone = bone, Normal = norm } end
function TOOL:GetEnt(i) return self.Objects[i] and self.Objects[i].Ent or NULL end
function TOOL:GetPos(i) local o = self.Objects[i]; if not o then return Vector() end return IsValid(o.Ent) and o.Ent:LocalToWorld(o.Pos) or o.Pos end
function TOOL:GetLocalPos(i) return self.Objects[i] and self.Objects[i].Pos or Vector() end
function TOOL:GetPhys(i) return self.Objects[i] and self.Objects[i].Phys end
function TOOL:GetBone(i) return self.Objects[i] and self.Objects[i].Bone or 0 end
function TOOL:GetNormal(i) return self.Objects[i] and self.Objects[i].Normal or Vector(0, 0, 1) end
function TOOL:CheckObjects() for _, o in pairs(self.Objects) do if not IsValid(o.Ent) then self:ClearObjects() return end end end
function TOOL:GetClientInfo(prop) local cv = GetConVar(tostring(self.Mode) .. "_" .. prop); return cv and cv:GetString() or "" end
function TOOL:GetClientNumber(prop, default) local cv = GetConVar(tostring(self.Mode) .. "_" .. prop); return cv and cv:GetFloat() or default or 0 end
function TOOL:GetClientBool(prop, default) local cv = GetConVar(tostring(self.Mode) .. "_" .. prop); if cv then return cv:GetBool() end return default == true end
function TOOL:GetServerInfo(prop) local cv = GetConVar(tostring(self.Mode) .. "_" .. prop); return cv and cv:GetString() or "" end
function TOOL:BuildConVarList() local out = {}; local m = tostring(self.Mode) .. "_"; for k, v in pairs(self.ClientConVar) do out[m .. k] = v end return out end
function TOOL:CreateConVars() for k, v in pairs(self.ClientConVar) do CreateConVar(tostring(self.Mode) .. "_" .. k, v, FCVAR_REPLICATED) end end
for _, n in ipairs({ "MakeGhostEntity", "ReleaseGhostEntity", "StartGhostEntity", "UpdateData", "UpdateGhostEntity" }) do if TOOL[n] == nil then TOOL[n] = stub("Tool:" .. n) end end
__glua_classes.Tool = TOOL
ToolObj = TOOL
local function tool_trace(ctx)
    local h = ctx.hit or {}
    local pos = h.pos and Vector(h.pos.x, h.pos.y, h.pos.z) or Vector()
    local n = h.normal and Vector(h.normal.x, h.normal.y, h.normal.z) or Vector(0, 0, 1)
    local world = (h.ent or 0) == 0
    return {
        Hit = true, HitPos = pos, HitNormal = n, Entity = world and NULL or Entity(h.ent), Fraction = 0.5, FractionLeftSolid = 0, StartPos = pos,
        HitWorld = world, HitNonWorld = not world, HitSky = false, HitNoDraw = false, HitBox = 0, HitGroup = 0,
        HitTexture = world and "**studio**" or "", MatType = 0, Normal = -n, PhysicsBone = 0, StartSolid = false, AllSolid = false,
        SurfaceProps = 0, SurfaceFlags = 0, DispFlags = 0, Contents = 0,
    }
end
-- @hook TOOL:LeftClick
-- @hook TOOL:RightClick
-- @hook TOOL:Reload
local native_has_tool, native_tool = __has_tool, __tool
function __has_tool(name) return native_has_tool(name) or glua_tools[name] ~= nil end
function __tool(name, button, ctx)
    if native_has_tool(name) then return native_tool(name, button, ctx) end
    local t = glua_tools[name]
    if t == nil then return nil end
    local f = button == "right" and t.RightClick or button == "reload" and t.Reload or t.LeftClick
    if f == nil then return nil end
    t.Owner = Player(ctx.player or 0)
    t.Stage = ctx.stage or 0
    if f(t, tool_trace(ctx)) == true then return { { op = "done" } } end
    return nil
end

duplicator = duplicator or {}
duplicator.EntityClasses, duplicator.EntityModifiers, duplicator.BoneModifiers, duplicator.ConstraintType = {}, {}, {}, {}
local dup_allowed, dup_local_pos, dup_local_ang = {}, nil, nil
function duplicator.RegisterEntityClass(cls, fn, ...) duplicator.EntityClasses[cls] = { Func = fn, Args = { ... } } end
function duplicator.FindEntityClass(cls) return duplicator.EntityClasses[cls] end
function duplicator.RegisterEntityModifier(name, fn) duplicator.EntityModifiers[name] = fn end
function duplicator.RegisterBoneModifier(name, fn) duplicator.BoneModifiers[name] = fn end
function duplicator.RegisterConstraint(name, fn, ...) duplicator.ConstraintType[name] = { Func = fn, Args = { ... } } end
function duplicator.Allow(cls) dup_allowed[cls] = true end
function duplicator.Disallow(cls) dup_allowed[cls] = nil end
function duplicator.IsAllowed(cls) return dup_allowed[cls] == true end
function duplicator.StoreEntityModifier(ent, name, data) local t = rawget(ent, "__vars"); t.EntityMods = t.EntityMods or {}; t.EntityMods[name] = table.Copy(data) end
function duplicator.ClearEntityModifier(ent, name) local t = rawget(ent, "__vars"); if t.EntityMods then t.EntityMods[name] = nil end end
function duplicator.StoreBoneModifier(ent, bone, name, data) local t = rawget(ent, "__vars"); t.BoneMods = t.BoneMods or {}; t.BoneMods[bone] = t.BoneMods[bone] or {}; t.BoneMods[bone][name] = table.Copy(data) end
function duplicator.SetLocalPos(p) dup_local_pos = p end
function duplicator.SetLocalAng(a) dup_local_ang = a end
function duplicator.CopyEntTable(ent)
    if not IsValid(ent) or ent:IsPlayer() then return nil end
    local si = scripted_ents.__instance(ent:EntIndex())
    if si and si.PreEntityCopy then si:PreEntityCopy() end
    local v = rawget(ent, "__vars") or {}
    local t = { Class = ent:GetClass(), Pos = ent:GetPos(), Angle = ent:GetAngles(), Model = ent:GetModel(), Skin = ent:GetSkin(), Name = ent:GetName(), EntityMods = table.Copy(v.EntityMods or {}), BoneMods = table.Copy(v.BoneMods or {}), PhysicsObjects = { [0] = { Pos = ent:GetPos(), Angle = ent:GetAngles(), Frozen = ent:GetMoveType() == MOVETYPE_NONE } } }
    if dup_local_pos then t.Pos = t.Pos - dup_local_pos end
    if si and si.OnEntityCopyTableFinish then si:OnEntityCopyTableFinish(t) end
    if si and si.PostEntityCopy then si:PostEntityCopy() end
    return t
end
function duplicator.CopyEnts(ents_) local out = { Entities = {}, Constraints = {}, Mins = Vector(math.huge, math.huge, math.huge), Maxs = Vector(-math.huge, -math.huge, -math.huge) }
    for _, e in ipairs(ents_) do local t = duplicator.CopyEntTable(e); if t then out.Entities[e:EntIndex()] = t; OrderVectors(out.Mins, t.Pos) end end
    duplicator.WorkoutSize(out) return out end
function duplicator.Copy(ent, tbl) tbl = tbl or { Entities = {}, Constraints = {} }; local t = duplicator.CopyEntTable(ent); if t then tbl.Entities[ent:EntIndex()] = t end; duplicator.WorkoutSize(tbl); return tbl end
function duplicator.WorkoutSize(tbl) local mn, mx = Vector(math.huge, math.huge, math.huge), Vector(-math.huge, -math.huge, -math.huge)
    for _, e in pairs(tbl.Entities or {}) do local p = e.Pos; mn.x, mn.y, mn.z = math.min(mn.x, p.x), math.min(mn.y, p.y), math.min(mn.z, p.z); mx.x, mx.y, mx.z = math.max(mx.x, p.x), math.max(mx.y, p.y), math.max(mx.z, p.z) end
    tbl.Mins, tbl.Maxs = mn, mx end
function duplicator.CreateEntityFromTable(ply, data)
    local reg = duplicator.EntityClasses[data.Class]
    if reg then local args = {}; for i, k in ipairs(reg.Args) do args[i] = k == "Data" and data or data[k] end return reg.Func(ply, unpack(args)) end
    local e = ents.Create(data.Class); if not IsValid(e) then return NULL end
    if data.Model then e:SetModel(data.Model) end; e:SetPos(data.Pos); e:SetAngles(data.Angle); if data.Skin then e:SetSkin(data.Skin) end; e:Spawn()
    return e
end
function duplicator.ApplyEntityModifiers(ply, ent) for name, data in pairs((rawget(ent, "__vars") or {}).EntityMods or {}) do if duplicator.EntityModifiers[name] then duplicator.EntityModifiers[name](ply, ent, data) end end end
function duplicator.ApplyBoneModifiers(ply, ent) for bone, mods in pairs((rawget(ent, "__vars") or {}).BoneMods or {}) do for name, data in pairs(mods) do if duplicator.BoneModifiers[name] then duplicator.BoneModifiers[name](ply, ent, bone, ent:GetPhysicsObject(), data) end end end end
function duplicator.GenericDuplicatorFunction(ply, data) local e = ents.Create(data.Class); if not IsValid(e) then return NULL end; duplicator.DoGeneric(e, data); e:Spawn(); return e end
function duplicator.DoGeneric(ent, data) if data.Model then ent:SetModel(data.Model) end; if data.Pos then ent:SetPos(data.Pos) end; if data.Angle then ent:SetAngles(data.Angle) end; if data.Skin then ent:SetSkin(data.Skin) end; if data.Name then ent:SetName(data.Name) end; if data.EntityMods then rawget(ent, "__vars").EntityMods = table.Copy(data.EntityMods) end end
function duplicator.DoGenericPhysics(ent, ply, data) local p = data.PhysicsObjects and data.PhysicsObjects[0]; if p then ent:SetPos(p.Pos); ent:SetAngles(p.Angle); if p.Frozen then ent:SetMoveType(MOVETYPE_NONE) end end end
function duplicator.Paste(ply, entities, constraints) local created = {}
    for id, data in pairs(entities) do local d = table.Copy(data); if dup_local_pos then d.Pos = d.Pos + dup_local_pos end; local e = duplicator.CreateEntityFromTable(ply, d); if IsValid(e) then created[id] = e; duplicator.ApplyEntityModifiers(ply, e); local si = scripted_ents.__instance(e:EntIndex()); if si then if si.OnDuplicated then si:OnDuplicated(d) end if si.PostEntityPaste then si:PostEntityPaste(ply, e, created) end end end end
    return created, {} end
for _, n in ipairs({ "CreateConstraintFromTable", "DoBoneManipulator", "DoFlex", "FigureOutRequiredAddons", "GetAllConstrainedEntitiesAndConstraints", "RemoveMapCreatedEntities" }) do if duplicator[n] == nil then duplicator[n] = stub("duplicator." .. n) end end

-- Classes the own server has no object for yet: every documented method is a stub.
for _, cls in ipairs({ "CUserCmd", "CMoveData", "CNavLadder", "PathFollower", "CLuaLocomotion" }) do
    local T = __glua_classes[cls] or {}
    T.__index = T
    __glua_classes[cls] = T
end
for _, n in ipairs({ "GetMouseWheel", "SetMouseWheel" }) do if __glua_classes.CUserCmd[n] == nil then __glua_classes.CUserCmd[n] = stub("CUserCmd:" .. n) end end
-- CUserCmd: a copy of the command the server processed for the player this tick
-- (Player:GetCurrentCommand); the command has already run, so writes change the copy only.
do
    local C = __glua_classes.CUserCmd
    function C:GetButtons() return self.buttons end
    function C:SetButtons(b) self.buttons = math.floor(tonumber(b) or 0) end
    function C:KeyDown(k) return bit.band(self.buttons, k) ~= 0 end
    function C:AddKey(k) self.buttons = bit.bor(self.buttons, k) end
    function C:RemoveKey(k) self.buttons = bit.band(self.buttons, bit.bnot(k)) end
    function C:ClearButtons() self.buttons = 0 end
    function C:ClearMovement() self.move = { x = 0, y = 0, z = 0 } end
    function C:GetForwardMove() return self.move.x end
    function C:SetForwardMove(v) self.move.x = tonumber(v) or 0 end
    function C:GetSideMove() return self.move.y end
    function C:SetSideMove(v) self.move.y = tonumber(v) or 0 end
    function C:GetUpMove() return self.move.z end
    function C:SetUpMove(v) self.move.z = tonumber(v) or 0 end
    function C:GetViewAngles() return Angle(self.view.x, self.view.y, self.view.z) end
    function C:SetViewAngles(a) if a then self.view = { x = a.p or a.x or 0, y = a.y or 0, z = a.r or a.z or 0 } end end
    function C:GetImpulse() return self.impulse end
    function C:SetImpulse(i) self.impulse = math.floor(tonumber(i) or 0) end
    function C:GetMouseX() return self.mx end
    function C:SetMouseX(v) self.mx = tonumber(v) or 0 end
    function C:GetMouseY() return self.my end
    function C:SetMouseY(v) self.my = tonumber(v) or 0 end
    function C:CommandNumber() return self.tick end
    function C:TickCount() return self.tick end
    function C:IsForced() return false end
    function C:SelectWeapon(w) if self.__ply and IsValid(w) then self.__ply:SelectWeapon(w:GetClass()) end end
end
for _, n in ipairs({ "GetAbsMoveAngles", "GetConstraintCenter", "GetConstraintRadius", "GetConstraintSpeedScale", "GetConstraintWidth", "GetFinalIdealVelocity", "GetFinalJumpVelocity", "GetFinalStepHeight", "SetAbsMoveAngles", "SetConstraintCenter", "SetConstraintRadius", "SetConstraintSpeedScale", "SetConstraintWidth", "SetFinalIdealVelocity", "SetFinalJumpVelocity", "SetFinalStepHeight" }) do if __glua_classes.CMoveData[n] == nil then __glua_classes.CMoveData[n] = stub("CMoveData:" .. n) end end
-- CMoveData: the move of a player's command as SetupMove / Move / FinishMove see it; origin,
-- velocity, buttons, moves, angles and max speed are written back to the server's mover.
do
    local M = __glua_classes.CMoveData
    local function v3(t) return Vector(t.x, t.y, t.z) end
    local function a3(t) return Angle(t.x, t.y, t.z) end
    local function setv(t, v) if v then t.x, t.y, t.z = v.x or 0, v.y or 0, v.z or 0 end end
    local function seta(t, a) if a then t.x, t.y, t.z = a.p or a.x or 0, a.y or 0, a.r or a.z or 0 end end
    function M:GetOrigin() return v3(self.origin) end
    function M:SetOrigin(v) setv(self.origin, v) end
    function M:GetVelocity() return v3(self.velocity) end
    function M:SetVelocity(v) setv(self.velocity, v) end
    function M:GetAngles() return a3(self.angles) end
    function M:SetAngles(a) seta(self.angles, a) end
    function M:GetMoveAngles() return a3(self.angles) end
    function M:SetMoveAngles(a) seta(self.angles, a) end
    function M:GetOldAngles() return a3(self.old_angles) end
    function M:SetOldAngles(a) seta(self.old_angles, a) end
    function M:GetButtons() return self.buttons end
    function M:SetButtons(b) self.buttons = math.floor(tonumber(b) or 0) end
    function M:GetOldButtons() return self.old_buttons end
    function M:SetOldButtons(b) self.old_buttons = math.floor(tonumber(b) or 0) end
    function M:KeyDown(k) return bit.band(self.buttons, k) ~= 0 end
    function M:KeyWasDown(k) return bit.band(self.old_buttons, k) ~= 0 end
    function M:KeyPressed(k) return self:KeyDown(k) and not self:KeyWasDown(k) end
    function M:KeyReleased(k) return self:KeyWasDown(k) and not self:KeyDown(k) end
    function M:AddKey(k) self.buttons = bit.bor(self.buttons, k) end
    function M:RemoveKey(k) self.buttons = bit.band(self.buttons, bit.bnot(k)) end
    function M:GetForwardSpeed() return self.moves.x end
    function M:SetForwardSpeed(v) self.moves.x = tonumber(v) or 0 end
    function M:GetSideSpeed() return self.moves.y end
    function M:SetSideSpeed(v) self.moves.y = tonumber(v) or 0 end
    function M:GetUpSpeed() return self.moves.z end
    function M:SetUpSpeed(v) self.moves.z = tonumber(v) or 0 end
    function M:GetMaxSpeed() return self.maxspeed end
    function M:SetMaxSpeed(v) self.maxspeed = math.max(tonumber(v) or 0, 0) end
    function M:GetMaxClientSpeed() return self.maxspeed end
    function M:SetMaxClientSpeed(v) self.maxspeed = math.max(tonumber(v) or 0, 0) end
    function M:GetImpulseCommand() return self.impulse end
    function M:SetImpulseCommand(i) self.impulse = math.floor(tonumber(i) or 0) end
    function M:__text()
        local o, v, m, a = self.origin, self.velocity, self.moves, self.angles
        return string.format("%.9g %.9g %.9g %.9g %.9g %.9g %d %.9g %.9g %.9g %.9g %.9g %.9g %.9g",
            o.x, o.y, o.z, v.x, v.y, v.z, self.buttons, m.x, m.y, m.z, a.x, a.y, a.z, self.maxspeed)
    end
end
for _, n in ipairs({ "ConnectTo", "Disconnect", "GetBottom", "GetBottomArea", "GetID", "GetLength", "GetNormal", "GetPosAtHeight", "GetTop", "GetTopBehindArea", "GetTopForwardArea", "GetTopLeftArea", "GetTopRightArea", "GetWidth", "IsConnectedAtSide", "IsValid", "Remove", "SetBottomArea", "SetTopBehindArea", "SetTopForwardArea", "SetTopLeftArea", "SetTopRightArea" }) do if __glua_classes.CNavLadder[n] == nil then __glua_classes.CNavLadder[n] = stub("CNavLadder:" .. n) end end
-- PathFollower: a polyline from our navmesh path query (nav.path) with the game's segment
-- table and cursor. Update / Chase need a NextBot's locomotion, Draw the client: stubs.
do
    local PF = __glua_classes.PathFollower
    local function load_points(self, pts)
        local segs, dist = {}, 0
        for i, pos in ipairs(pts) do
            local nxt = pts[i + 1]
            local len = nxt and pos:Distance(nxt) or 0
            local fwd = nxt and (nxt - pos):GetNormalized() or (segs[i - 1] and segs[i - 1].forward or Vector(1, 0, 0))
            segs[i] = {
                pos = pos, length = len, distanceFromStart = dist, forward = fwd, type = 0, how = 0,
                curvature = 0, area = (navmesh and navmesh.GetNavArea) and navmesh.GetNavArea(pos, 120) or NULL,
                ladder = NULL, m_portalCenter = pos, m_portalHalfWidth = 0,
            }
            dist = dist + len
        end
        self.segs, self.length, self.cursor, self.age_at = segs, dist, 0, CurTime()
    end
    function PF:__load(pts) load_points(self, pts) return #pts > 0 end
    function PF:Compute(bot, goal, costfn)
        self.segs, self.length, self.cursor = {}, 0, 0
        if not IsValid(bot) or goal == nil then return false end
        local r = nav.path(bot:GetPos(), goal)
        if not r or not r.ok then return false end
        local pts = {}
        for i, q in ipairs(r.points) do pts[i] = Vector(q.x, q.y, q.z) end
        load_points(self, pts)
        self.partial = r.partial
        return #pts > 0 and not r.partial
    end
    function PF:IsValid() return self.segs ~= nil and #self.segs > 0 end
    function PF:Invalidate() self.segs, self.length, self.cursor = {}, 0, 0 end
    function PF:GetAllSegments() return self.segs or {} end
    function PF:FirstSegment() return self.segs and self.segs[1] or nil end
    function PF:LastSegment() return self.segs and self.segs[#self.segs] or nil end
    function PF:GetLength() return self.length or 0 end
    function PF:GetStart() local s = self:FirstSegment(); return s and s.pos or Vector() end
    function PF:GetEnd() local s = self:LastSegment(); return s and s.pos or Vector() end
    function PF:GetAge() return self.age_at and (CurTime() - self.age_at) or 0 end
    function PF:ResetAge() self.age_at = CurTime() end
    function PF:GetGoalTolerance() return self.tolerance or 0 end
    function PF:SetGoalTolerance(t) self.tolerance = tonumber(t) or 0 end
    function PF:GetMinLookAheadDistance() return self.lookahead or 0 end
    function PF:SetMinLookAheadDistance(d) self.lookahead = tonumber(d) or 0 end
    local function seg_at(self, dist)
        local segs = self.segs or {}
        for i = #segs, 1, -1 do if dist >= segs[i].distanceFromStart then return i end end
        return 1
    end
    function PF:GetPositionOnPath(dist)
        local segs = self.segs or {}
        if #segs == 0 then return Vector() end
        dist = math.max(0, math.min(dist or 0, self.length or 0))
        local i = seg_at(self, dist)
        local s = segs[i]
        if s.length <= 0 then return s.pos end
        return s.pos + s.forward * math.min(dist - s.distanceFromStart, s.length)
    end
    function PF:MoveCursorTo(dist) self.cursor = math.max(0, math.min(tonumber(dist) or 0, self.length or 0)) end
    function PF:MoveCursor(delta) self:MoveCursorTo((self.cursor or 0) + (tonumber(delta) or 0)) end
    function PF:MoveCursorToStart() self.cursor = 0 end
    function PF:MoveCursorToEnd() self.cursor = self.length or 0 end
    function PF:GetCursorPosition() return self.cursor or 0 end
    function PF:GetCursorData()
        local segs = self.segs or {}
        if #segs == 0 then return { curvature = 0, forward = Vector(1, 0, 0), pos = Vector() } end
        local s = segs[seg_at(self, self.cursor or 0)]
        return { curvature = 0, forward = s.forward, pos = self:GetPositionOnPath(self.cursor or 0) }
    end
    local function closest(self, pos)
        local segs = self.segs or {}
        local best, bd, bdist = Vector(), math.huge, 0
        for i = 1, #segs do
            local s = segs[i]
            local q, t = s.pos, 0
            if s.length > 0 then
                t = math.max(0, math.min(s.length, (pos - s.pos):Dot(s.forward)))
                q = s.pos + s.forward * t
            end
            local d = q:DistToSqr(pos)
            if d < bd then best, bd, bdist = q, d, s.distanceFromStart + t end
        end
        return best, bdist
    end
    function PF:GetClosestPosition(pos) local q = closest(self, pos); return q end
    function PF:MoveCursorToClosestPosition(pos) local _, d = closest(self, pos); self.cursor = d end
    function PF:GetCurrentGoal()
        local segs = self.segs or {}
        if #segs == 0 then return nil end
        local i = seg_at(self, self.cursor or 0)
        return segs[math.min(i + 1, #segs)]
    end
    function PF:NextSegment() local segs = self.segs or {}; return segs[math.min(seg_at(self, self.cursor or 0) + 1, #segs)] end
    function PF:PriorSegment() local segs = self.segs or {}; return segs[math.max(seg_at(self, self.cursor or 0) - 1, 1)] end
end
local PF = __glua_classes.PathFollower
-- Update: drive the bot's locomotion toward the current goal, advancing the cursor as segments
-- are reached (goal tolerance); Chase: recompute toward a moving entity, then Update
function PF:Update(bot)
    if not bot or not bot.loco or #self.segs == 0 then return end
    local pos = bot:GetPos()
    local goal = self:GetCurrentGoal()
    if not goal then return end
    local tol = self.tolerance or 20
    while goal and pos:Distance(Vector(goal.pos.x, goal.pos.y, pos.z)) <= tol do
        self.cursor = math.min((self.cursor or 0) + (goal.length or tol), self:GetLength())
        if self.cursor >= self:GetLength() then self.segs = {} return end
        goal = self:GetCurrentGoal()
    end
    if goal then bot.loco:Approach(goal.pos, 1); bot.loco:FaceTowards(goal.pos) end
end
function PF:Chase(bot, ent)
    if not IsValid(ent) then return end
    local target = ent:GetPos()
    local last = rawget(self, "__chase_target")
    if not last or last:Distance(target) > 30 or #self.segs == 0 then
        rawset(self, "__chase_target", target)
        self:Compute(bot, target)
    end
    self:Update(bot)
end
function Path(kind) return setmetatable({ kind = kind or "Follow", segs = {}, length = 0, cursor = 0 }, __glua_classes.PathFollower) end
for _, n in ipairs({ "Compute", "Draw", "FirstSegment", "GetAge", "GetAllSegments", "GetClosestPosition", "GetCurrentGoal", "GetCursorData", "GetCursorPosition", "GetEnd", "GetGoalTolerance", "GetHindrance", "GetLength", "GetMinLookAheadDistance", "GetPositionOnPath", "GetStart", "Invalidate", "IsValid", "LastSegment", "MoveCursor", "MoveCursorTo", "MoveCursorToClosestPosition", "MoveCursorToEnd", "MoveCursorToStart", "NextSegment", "PriorSegment", "ResetAge", "SetGoalTolerance", "SetMinLookAheadDistance"}) do if __glua_classes.PathFollower[n] == nil then __glua_classes.PathFollower[n] = stub("PathFollower:" .. n) end end
for _, n in ipairs({ "GetCurrentAcceleration", "GetNextBot", "IsUsingLadder"}) do if __glua_classes.CLuaLocomotion[n] == nil then __glua_classes.CLuaLocomotion[n] = stub("CLuaLocomotion:" .. n) end end
drive = drive or {}
for _, n in ipairs({ "CalcView", "CreateMove", "DestroyMethod", "End", "FinishMove", "GetMethod", "Move", "PlayerStartDriving", "PlayerStopDriving", "Register", "Start", "StartMove" }) do if drive[n] == nil then drive[n] = stub("drive." .. n) end end

-- KEY_ enum (documented button codes; the numpad library keys on them)
KEY_NONE = 0
KEY_0 = 1
KEY_1 = 2
KEY_2 = 3
KEY_3 = 4
KEY_4 = 5
KEY_5 = 6
KEY_6 = 7
KEY_7 = 8
KEY_8 = 9
KEY_9 = 10
KEY_A = 11
KEY_B = 12
KEY_C = 13
KEY_D = 14
KEY_E = 15
KEY_F = 16
KEY_G = 17
KEY_H = 18
KEY_I = 19
KEY_J = 20
KEY_K = 21
KEY_L = 22
KEY_M = 23
KEY_N = 24
KEY_O = 25
KEY_P = 26
KEY_Q = 27
KEY_R = 28
KEY_S = 29
KEY_T = 30
KEY_U = 31
KEY_V = 32
KEY_W = 33
KEY_X = 34
KEY_Y = 35
KEY_Z = 36
KEY_PAD_0 = 37
KEY_PAD_1 = 38
KEY_PAD_2 = 39
KEY_PAD_3 = 40
KEY_PAD_4 = 41
KEY_PAD_5 = 42
KEY_PAD_6 = 43
KEY_PAD_7 = 44
KEY_PAD_8 = 45
KEY_PAD_9 = 46
KEY_PAD_DIVIDE = 47
KEY_PAD_MULTIPLY = 48
KEY_PAD_MINUS = 49
KEY_PAD_PLUS = 50
KEY_PAD_ENTER = 51
KEY_PAD_DECIMAL = 52
KEY_LBRACKET = 53
KEY_RBRACKET = 54
KEY_SEMICOLON = 55
KEY_APOSTROPHE = 56
KEY_BACKQUOTE = 57
KEY_COMMA = 58
KEY_PERIOD = 59
KEY_SLASH = 60
KEY_BACKSLASH = 61
KEY_MINUS = 62
KEY_EQUAL = 63
KEY_ENTER = 64
KEY_SPACE = 65
KEY_BACKSPACE = 66
KEY_TAB = 67
KEY_CAPSLOCK = 68
KEY_NUMLOCK = 69
KEY_ESCAPE = 70
KEY_SCROLLLOCK = 71
KEY_INSERT = 72
KEY_DELETE = 73
KEY_HOME = 74
KEY_END = 75
KEY_PAGEUP = 76
KEY_PAGEDOWN = 77
KEY_BREAK = 78
KEY_LSHIFT = 79
KEY_RSHIFT = 80
KEY_LALT = 81
KEY_RALT = 82
KEY_LCONTROL = 83
KEY_RCONTROL = 84
KEY_LWIN = 85
KEY_RWIN = 86
KEY_APP = 87
KEY_UP = 88
KEY_LEFT = 89
KEY_DOWN = 90
KEY_RIGHT = 91
KEY_F1 = 92
KEY_F2 = 93
KEY_F3 = 94
KEY_F4 = 95
KEY_F5 = 96
KEY_F6 = 97
KEY_F7 = 98
KEY_F8 = 99
KEY_F9 = 100
KEY_F10 = 101
KEY_F11 = 102
KEY_F12 = 103
KEY_CAPSLOCKTOGGLE = 104
KEY_NUMLOCKTOGGLE = 105
KEY_SCROLLLOCKTOGGLE = 106
KEY_LAST = KEY_SCROLLLOCKTOGGLE
KEY_COUNT = KEY_LAST + 1

-- PLAYER class callbacks run through player_manager on the matching gamemode hooks.
-- @hook PLAYER:Init
-- @hook PLAYER:Spawn
-- @hook PLAYER:Loadout
-- @hook PLAYER:Death
-- @hook PLAYER:SetModel
-- @hook PLAYER:ClassChanged
hook.Add("PlayerInitialSpawn", "__player_manager", function(ply) player_manager.RunClass(ply, "Init") end)
hook.Add("PlayerSpawn", "__player_manager", function(ply, transition) player_manager.OnPlayerSpawn(ply, transition) end)
hook.Add("PlayerLoadout", "__player_manager", function(ply) player_manager.RunClass(ply, "Loadout") end)
hook.Add("PlayerDeath", "__player_manager", function(ply) player_manager.RunClass(ply, "Death") end)
hook.Add("PlayerSetModel", "__player_manager", function(ply) player_manager.RunClass(ply, "SetModel") end)

-- Documented callbacks the server never calls yet (AI schedules / tasks, movement, animation,
-- client-side effects): acknowledged as stubs, not claimed.
for _, n in ipairs({ "CalcAbsolutePosition", "CanEditVariables", "CreateSchedulesInternal", "DoImpactEffect", "DoingEngineSchedule", "Draw", "DrawTranslucent", "EngineScheduleFinish", "ExpressionFinished", "FireAnimationEvent", "GetAttackSpread", "GetNPCClass", "GetPreferredCarryAngles", "GetRelationship", "GetRenderMesh", "GetShadowCastDirection", "GetSoundInterests", "GravGunPickupAllowed", "GravGunPunt", "HandleAnimEvent", "ImpactTrace", "OnChangeActiveWeapon", "OnChangeActivity", "OnCondition", "OnReloaded", "OnRestore", "OnTaskComplete", "OnTaskFailed", "OnTraceAttack", "OverrideMove", "OverrideMoveFacing", "PassesTriggerFilters", "RenderOverride", "ResolveCustomFlyCollision", "RunEngineTask", "SetAutomaticFrameAdvance", "SetNPCClass", "StartEngineSchedule", "StartEngineTask", "TestCollision", "TranslateActivity", "TranslateNavGoal", "TranslateSchedule"}) do stub("ENTITY:" .. n) end
for _, n in ipairs({ "AddDeathNotice", "AdjustMouseSensitivity", "CalcMainActivity", "CalcVehicleView", "CalcView", "CalcViewModelView", "CaptureVideo", "ChatText", "ChatTextChanged", "ClientSignOnStateChanged", "CloseDermaMenus", "CreateClientsideRagdoll", "CreateEntityRagdoll", "CreateMove", "DoAnimationEvent", "DrawDeathNotice", "DrawMonitors", "DrawOverlay", "DrawPhysgunBeam", "EndEntityDriving", "FinishChat", "ForceDermaSkin", "GUIMouseDoublePressed", "GUIMousePressed", "GUIMouseReleased", "GameContentChanged", "GetDeathNoticeEntityName", "GetMotionBlurValues", "GetTeamColor", "GetTeamNumColor", "GrabEarAnimation", "GravGunPunt", "HUDAmmoPickedUp", "HUDDrawPickupHistory", "HUDDrawScoreBoard", "HUDDrawTargetID", "HUDItemPickedUp", "HUDPaint", "HUDPaintBackground", "HUDShouldDraw", "HUDWeaponPickedUp", "HandlePlayerArmorReduction", "HandlePlayerDriving", "HandlePlayerDucking", "HandlePlayerJumping", "HandlePlayerLanding", "HandlePlayerNoClipping", "HandlePlayerSwimming", "HandlePlayerVaulting", "HideTeam", "InputMouseApply", "LoadGModSave", "LoadGModSaveFailed", "MenuStart", "MouthMoveAnimation", "NeedsDepthPass", "NetworkEntityCreated", "NotifyShouldTransmit", "OnChatTab", "OnClientLuaError", "OnCloseCaptionEmit", "OnContextMenuClose", "OnContextMenuOpen", "OnNotifyAddonConflict", "OnPauseMenuBlockedTooManyTimes", "OnPauseMenuShow", "OnPermissionsChanged", "OnPlayerChat", "OnScreenSizeChanged", "OnSpawnMenuClose", "OnSpawnMenuOpen", "OnTextEntryGetFocus", "OnTextEntryLoseFocus", "OnViewModelChanged", "PlayerBindPress", "PlayerButtonDown", "PlayerButtonUp", "PlayerDriveAnimate", "PlayerFireAnimationEvent", "PlayerHandleAnimEvent", "PopulateMenuBar", "PostDraw2DSkyBox", "PostDrawEffects", "PostDrawHUD", "PostDrawOpaqueRenderables", "PostDrawPlayerHands", "PostDrawSkyBox", "PostDrawTranslucentRenderables", "PostDrawViewModel", "PostPlayerDraw", "PostProcessPermitted", "PostRender", "PostRenderVGUI", "PreDrawEffects", "PreDrawHUD", "PreDrawHalos", "PreDrawOpaqueRenderables", "PreDrawPlayerHands", "PreDrawSkyBox", "PreDrawTranslucentRenderables", "PreDrawViewModel", "PreDrawViewModels", "PrePlayerDraw", "PreRender", "PreventScreenClicks", "RenderScene", "RenderScreenspaceEffects", "ScoreboardHide", "ScoreboardShow", "SendDeathNotice", "SetupSkyboxFog", "SetupWorldFog", "ShouldDrawLocalPlayer", "ShowHelp", "ShowSpare1", "ShowSpare2", "ShowTeam", "SpawnMenuCreated", "SpawniconGenerated", "StartChat", "StartCommand", "StartEntityDriving", "StartGame", "TranslateActivity", "UpdateAnimation", "VGUIMousePressAllowed", "VGUIMousePressed", "VehicleMove", "WorkshopDownloadFile", "WorkshopDownloadProgress", "WorkshopDownloadTotals", "WorkshopDownloadedFile", "WorkshopEnd", "WorkshopExtractProgress", "WorkshopStart", "WorkshopSubscriptionsChanged", "WorkshopSubscriptionsMessage", "WorkshopSubscriptionsProgress" }) do stub("GM:" .. n) end
for _, n in ipairs({ "BodyUpdate", "OnEntitySight", "OnEntitySightLost", "OnIgnite", "OnKilled", "OnOtherKilled", "OnTraceAttack" }) do stub("NEXTBOT:" .. n) end
for _, n in ipairs({ "FinishMove", "GetHandsModel", "Move", "PostDrawViewModel", "PreDrawViewModel", "SetupDataTables", "StartMove", "ViewModelChanged" }) do stub("PLAYER:" .. n) end
for _, n in ipairs({ "AddGamemodeToolMenuCategories", "AddGamemodeToolMenuTabs", "AddToolMenuCategories", "AddToolMenuTabs", "CanArmDupe", "CanDrive", "ContentSidebarSelection", "ContextMenuClosed", "ContextMenuCreated", "ContextMenuEnabled", "ContextMenuOpen", "ContextMenuOpened", "ContextMenuShowTool", "OnRevertSpawnlist", "OnSaveSpawnlist", "OpenToolbox", "PaintNotes", "PaintWorldTips", "PersistenceLoad", "PersistenceSave", "PlayerSpawnEffect", "PlayerSpawnObject", "PlayerSpawnedEffect", "PopulateContent", "PopulateEntities", "PopulateNPCs", "PopulatePropMenu", "PopulateToolMenu", "PopulateVehicles", "PopulateWeapons", "PostReloadToolsMenu", "PreRegisterTOOL", "PreReloadToolsMenu", "SpawnMenuEnabled", "SpawnMenuOpen", "SpawnMenuOpened", "SpawnlistContentChanged", "SpawnlistOpenGenericMenu", "SpawnmenuIconMenuOpen" }) do stub("SANDBOX:" .. n) end
for _, n in ipairs({ "BuildCPanel", "Deploy", "DrawHUD", "DrawToolScreen", "FreezeMovement", "Holster", "Think" }) do stub("TOOL:" .. n) end
for _, n in ipairs({ "AcceptInput", "AdjustMouseSensitivity", "CalcView", "CalcViewModelView", "CustomAmmoDisplay", "DoDrawCrosshair", "DoImpactEffect", "DrawHUD", "DrawHUDBackground", "DrawWeaponSelection", "DrawWorldModel", "DrawWorldModelTranslucent", "FireAnimationEvent", "FreezeMovement", "GetCapabilities", "GetNPCBurstSettings", "GetNPCRestTimes", "GetTracerOrigin", "GetViewModelPosition", "HUDShouldDraw", "KeyValue", "NPCShoot_Secondary", "OnRestore", "PostDrawViewModel", "PreDrawViewModel", "PrintWeaponInfo", "RenderScreen", "ShootEffects", "ShouldDrawViewModel", "TranslateActivity", "TranslateFOV", "ViewModelDrawn" }) do stub("WEAPON:" .. n) end


-- Shared / server tail. Stack and CSoundPatch are own objects; cookie persists through the addon's
-- file storage; properties / gamemode are registries; everything that needs sqlite, the engine's
-- save system, materials, the AI graph or a debug backend is an acknowledged stub.
local STACK = {}
STACK.__index = STACK
function STACK:Push(v) self.n = self.n + 1; self[self.n] = v end
function STACK:Pop(n) local out = {}; for i = 1, n or 1 do if self.n == 0 then break end out[i] = self[self.n]; self[self.n] = nil; self.n = self.n - 1 end return unpack(out) end
function STACK:PopMulti(n) local out = {}; for i = 1, n or 1 do if self.n == 0 then break end out[i] = self[self.n]; self[self.n] = nil; self.n = self.n - 1 end return out end
function STACK:Size() return self.n end
function STACK:Top() return self[self.n] end
function util.Stack() return setmetatable({ n = 0 }, STACK) end
__glua_classes.Stack = STACK

local SP = {}
SP.__index = SP
function CreateSound(ent, name, filter) return setmetatable({ ent = ent, name = name, filter = filter, pitch = 100, volume = 1, level = 75, dsp = 0, playing = false }, SP) end
function SP:Play() self.playing = true; local pos = IsValid(self.ent) and self.ent:GetPos() or Vector(); sound.Play(self.name, pos, self.level, self.pitch, self.volume) end
function SP:PlayEx(vol, pitch) self.volume, self.pitch = vol or self.volume, pitch or self.pitch; self:Play() end
function SP:IsPlaying() return self.playing end
function SP:GetSoundName() return self.name end
function SP:GetPitch() return self.pitch end
function SP:GetVolume() return self.volume end
function SP:GetSoundLevel() return self.level end
function SP:GetDSP() return self.dsp end
function SP:ChangePitch(p, dt) self.pitch = p end
function SP:ChangeVolume(v, dt) self.volume = v end
function SP:SetDSP(d) self.dsp = d end
function SP:SetSoundLevel(l) self.level = l end
__glua_classes.CSoundPatch = SP

properties = properties or {}
properties.List = properties.List or {}
function properties.Add(name, data) data.InternalName = name; properties.List[string.lower(name)] = data end
function properties.Remove(name) properties.List[string.lower(name)] = nil end
function properties.CanBeTargeted(ent, ply) if not IsValid(ent) or ent:IsPlayer() then return false end if ent.CanProperty then return ent:CanProperty(ply, "") ~= false end return true end

cookie = cookie or {}
local cookies
local function cookie_load() if not cookies then local raw = file.Read("cookies.json", "DATA"); cookies = raw and util.JSONToTable(raw) or {} end return cookies end
local function cookie_save() file.Write("cookies.json", util.TableToJSON(cookies)) end
function cookie.Set(k, v) cookie_load()[k] = tostring(v); cookie_save() end
function cookie.GetString(k, default) local v = cookie_load()[k]; if v == nil then return default end return v end
function cookie.GetNumber(k, default) local v = tonumber(cookie_load()[k]); if v == nil then return default end return v end
function cookie.Delete(k) cookie_load()[k] = nil; cookie_save() end

gamemode = gamemode or {}
local gamemodes = {}
function gamemode.Register(gm, name, base) gm.Name = gm.Name or name; gm.FolderName = name; gm.BaseClass = gamemodes[base]; if gm.BaseClass then setmetatable(gm, { __index = gm.BaseClass }) end gamemodes[name] = gm; return true end
function gamemode.Get(name) return gamemodes[name] end
function gamemode.Call(name, ...) local fn = GAMEMODE and GAMEMODE[name]; if fn then return fn(GAMEMODE, ...) end end
gmod = gmod or {}
function gmod.GetGamemode() return GAMEMODE end

ai_schedule = ai_schedule or {}
ai_task = ai_task or {}
local SCHED, TASKT = {}, {}
SCHED.__index, TASKT.__index = SCHED, TASKT
function ai_schedule.New(name) return setmetatable({ Name = name, Tasks = {}, TaskCount = 0 }, SCHED) end
function ai_task.New() return setmetatable({ TaskName = "", TaskData = nil, RunFunc = nil, StartFunc = nil }, TASKT) end
function SCHED:AddTask(name, data) self.TaskCount = self.TaskCount + 1; local t = ai_task.New(); t.TaskName = name; t.TaskData = data; self.Tasks[self.TaskCount] = t end
function SCHED:EngTask(name, data) self:AddTask(name, data) end
function SCHED:NumTasks() return self.TaskCount end
function SCHED:GetTask(i) return self.Tasks[i] end
function TASKT:InitFunctionName(start, run, startData, runData) self.StartFunc, self.RunFunc, self.StartData, self.RunData = start, run, startData, runData end
function TASKT:InitEngine(name, data) self.TaskName, self.TaskData = name, data end
function SCHED:Init(name) self.Name = name; self.Tasks = {}; self.TaskCount = 0 end
-- a Lua task: `start` / `run` are method names on the NPC, both get `data`
function SCHED:AddTaskEx(start, run, data) self.TaskCount = self.TaskCount + 1; local t = ai_task.New(); t:InitFunctionName(start, run, data, data); self.Tasks[self.TaskCount] = t end
function TASKT:Init() self.TaskName, self.TaskData, self.RunFunc, self.StartFunc = "", nil, nil, nil end
function TASKT:IsEngineType() return self.StartFunc == nil and self.RunFunc == nil end
function TASKT:IsFNameType() return self.StartFunc ~= nil or self.RunFunc ~= nil end
function TASKT:Start_FName(npc) local f = self.StartFunc and npc[self.StartFunc]; if f then f(npc, self.StartData) end end
function TASKT:Run_FName(npc) local f = self.RunFunc and npc[self.RunFunc]; if f then f(npc, self.RunData) end end
function TASKT:Start(npc) if self:IsFNameType() then self:Start_FName(npc) end end
function TASKT:Run(npc) if self:IsFNameType() then self:Run_FName(npc) end end
TASKSTATUS_NEW, TASKSTATUS_RUNNING, TASKSTATUS_COMPLETE = 0, 1, 2
__glua_classes.Schedule = SCHED
__glua_classes.Task = TASKT

do
    local D = __glua_classes.CTakeDamageInfo
    if D then
        function D:GetWeapon() return rawget(self, "__weapon") or NULL end
        function D:SetWeapon(w) rawset(self, "__weapon", w) end
    end
    local RF = __glua_classes.CRecipientFilter
    if RF and RF.AddPlayer then
        function RF:AddPlayers(t) for _, p in ipairs(t) do self:AddPlayer(p) end end
        function RF:RemovePlayers(t) for _, p in ipairs(t) do self:RemovePlayer(p) end end
    end
    if net then
        net.Receivers = net.Receivers or {}
        net.WriteVars = net.WriteVars or { [TYPE_NIL] = function() end, [TYPE_STRING] = net.WriteString, [TYPE_NUMBER] = net.WriteDouble, [TYPE_TABLE] = net.WriteTable, [TYPE_BOOL] = net.WriteBool, [TYPE_ENTITY] = net.WriteEntity, [TYPE_VECTOR] = net.WriteVector, [TYPE_ANGLE] = net.WriteAngle, [TYPE_COLOR] = net.WriteColor }
        net.ReadVars = net.ReadVars or { [TYPE_NIL] = function() return nil end, [TYPE_STRING] = net.ReadString, [TYPE_NUMBER] = net.ReadDouble, [TYPE_TABLE] = net.ReadTable, [TYPE_BOOL] = net.ReadBool, [TYPE_ENTITY] = net.ReadEntity, [TYPE_VECTOR] = net.ReadVector, [TYPE_ANGLE] = net.ReadAngle, [TYPE_COLOR] = net.ReadColor }
    end
    local E = __glua_classes.Entity
    if E and E.Use == nil then function E:Use(activator, caller, useType, value) return self:Input("Use", activator, caller, value) end end
end
__glua_classes["CSoundPatch"] = __glua_classes["CSoundPatch"] or {}
for _, n in ipairs({ "FadeOut", "Stop" }) do if __glua_classes["CSoundPatch"][n] == nil then __glua_classes["CSoundPatch"][n] = stub("CSoundPatch:" .. n) end end
__glua_classes["IMaterial"] = __glua_classes["IMaterial"] or {}
for _, n in ipairs({ "GetColor", "GetFloat", "GetInt", "GetKeyValues", "GetMatrix", "GetName", "GetShader", "GetString", "GetTexture", "GetVector", "GetVector4D", "GetVectorLinear", "Height", "IsError", "Recompute", "SetFloat", "SetInt", "SetMatrix", "SetShader", "SetString", "SetTexture", "SetUndefined", "SetVector", "SetVector4D", "Width" }) do if __glua_classes["IMaterial"][n] == nil then __glua_classes["IMaterial"][n] = stub("IMaterial:" .. n) end end
__glua_classes["IRestore"] = __glua_classes["IRestore"] or {}
for _, n in ipairs({ "EndBlock", "ReadAngle", "ReadBool", "ReadEntity", "ReadFloat", "ReadInt", "ReadString", "ReadVector", "StartBlock" }) do if __glua_classes["IRestore"][n] == nil then __glua_classes["IRestore"][n] = stub("IRestore:" .. n) end end
__glua_classes["ISave"] = __glua_classes["ISave"] or {}
for _, n in ipairs({ "EndBlock", "StartBlock", "WriteAngle", "WriteBool", "WriteEntity", "WriteFloat", "WriteInt", "WriteString", "WriteVector" }) do if __glua_classes["ISave"][n] == nil then __glua_classes["ISave"][n] = stub("ISave:" .. n) end end
__glua_classes["ITexture"] = __glua_classes["ITexture"] or {}
for _, n in ipairs({ "Download", "GetColor", "GetMappingHeight", "GetMappingWidth", "GetName", "GetNumAnimationFrames", "Height", "IsError", "IsErrorTexture", "Width" }) do if __glua_classes["ITexture"][n] == nil then __glua_classes["ITexture"][n] = stub("ITexture:" .. n) end end
__glua_classes["NextBot"] = __glua_classes["NextBot"] or {}
for _, n in ipairs({ "BecomeRagdoll", "FindSpot", "FindSpots", "GetSolidMask", "SetSolidMask"}) do if __glua_classes["NextBot"][n] == nil then __glua_classes["NextBot"][n] = stub("NextBot:" .. n) end end
__glua_classes["PhysCollide"] = __glua_classes["PhysCollide"] or {}
for _, n in ipairs({ "Destroy", "IsValid", "TraceBox" }) do if __glua_classes["PhysCollide"][n] == nil then __glua_classes["PhysCollide"][n] = stub("PhysCollide:" .. n) end end
__glua_classes["Schedule"] = __glua_classes["Schedule"] or {}
for _, n in ipairs({ "AddTask", "EngTask", "GetTask", "NumTasks" }) do if __glua_classes["Schedule"][n] == nil then __glua_classes["Schedule"][n] = stub("Schedule:" .. n) end end
__glua_classes["SurfaceInfo"] = __glua_classes["SurfaceInfo"] or {}
for _, n in ipairs({ "GetMaterial", "GetVertices", "IsNoDraw", "IsSky", "IsWater" }) do if __glua_classes["SurfaceInfo"][n] == nil then __glua_classes["SurfaceInfo"][n] = stub("SurfaceInfo:" .. n) end end
__glua_classes["Task"] = __glua_classes["Task"] or {}
for _, n in ipairs({ "InitEngine", "InitFunctionName"}) do if __glua_classes["Task"][n] == nil then __glua_classes["Task"][n] = stub("Task:" .. n) end end
__glua_classes["WorkshopFileBase"] = __glua_classes["WorkshopFileBase"] or {}
for _, n in ipairs({ "Fetch", "FetchSubscribed", "FillFileInfo", "Publish", "RetrieveUserName" }) do if __glua_classes["WorkshopFileBase"][n] == nil then __glua_classes["WorkshopFileBase"][n] = stub("WorkshopFileBase:" .. n) end end
ai = ai or {}
for _, n in ipairs({ "GetNodeCount", "GetScheduleID", "GetSquadLeader", "GetSquadMemberCount", "GetSquadMembers", "GetTaskID" }) do if ai[n] == nil then ai[n] = stub("ai." .. n) end end
construct = construct or {}
for _, n in ipairs({ "Magnet", "SetPhysProp" }) do if construct[n] == nil then construct[n] = stub("construct." .. n) end end
coroutine = coroutine or {}
for _, n in ipairs({ "wait" }) do if coroutine[n] == nil then coroutine[n] = stub("coroutine." .. n) end end
debug = debug or {}
for _, n in ipairs({ "Trace", "debug", "getfenv", "gethook", "getinfo", "getlocal", "getmetatable", "getregistry", "getupvalue", "setfenv", "sethook", "setlocal", "setmetatable", "setupvalue", "traceback", "upvalueid", "upvaluejoin" }) do if debug[n] == nil then debug[n] = stub("debug." .. n) end end
debugoverlay = debugoverlay or {}
for _, n in ipairs({ "Axis", "Box", "BoxAngles", "Cross", "EntityTextAtPosition", "Grid", "Line", "ScreenText", "Sphere", "SweptBox", "Text", "Triangle" }) do if debugoverlay[n] == nil then debugoverlay[n] = stub("debugoverlay." .. n) end end
ents = ents or {}
for _, n in ipairs({ "FindInPVS", "FireTargets" }) do if ents[n] == nil then ents[n] = stub("ents." .. n) end end
gameevent = gameevent or {}
for _, n in ipairs({ "Listen" }) do if gameevent[n] == nil then gameevent[n] = stub("gameevent." .. n) end end
gmsave = gmsave or {}
for _, n in ipairs({ "LoadMap", "PlayerLoad", "PlayerSave", "SaveMap", "ShouldSaveEntity" }) do if gmsave[n] == nil then gmsave[n] = stub("gmsave." .. n) end end
hammer = hammer or {}
for _, n in ipairs({ "SendCommand" }) do if hammer[n] == nil then hammer[n] = stub("hammer." .. n) end end
http = http or {}
for _, n in ipairs({ "Fetch", "Post" }) do if http[n] == nil then http[n] = stub("http." .. n) end end
jit = jit or {}
for _, n in ipairs({ "arch", "attach", "flush", "off", "on", "os", "status", "version", "version_num" }) do if jit[n] == nil then jit[n] = stub("jit." .. n) end end
motionsensor = motionsensor or {}
for _, n in ipairs({ "BuildSkeleton", "ChooseBuilderFromEntity", "ProcessAngle", "ProcessAnglesTable", "ProcessPositionTable" }) do if motionsensor[n] == nil then motionsensor[n] = stub("motionsensor." .. n) end end
player = player or {}
for _, n in ipairs({ "GetCountConnecting" }) do if player[n] == nil then player[n] = stub("player." .. n) end end
properties = properties or {}
for _, n in ipairs({ "OnScreenClick", "OpenEntityMenu" }) do if properties[n] == nil then properties[n] = stub("properties." .. n) end end
resource = resource or {}
for _, n in ipairs({ "AddFile", "AddSingleFile", "AddWorkshop" }) do if resource[n] == nil then resource[n] = stub("resource." .. n) end end
saverestore = saverestore or {}
for _, n in ipairs({ "AddRestoreHook", "AddSaveHook", "LoadEntity", "LoadGlobal", "PreRestore", "PreSave", "ReadTable", "ReadVar", "SaveEntity", "SaveGlobal", "WritableKeysInTable", "WriteTable", "WriteVar" }) do if saverestore[n] == nil then saverestore[n] = stub("saverestore." .. n) end end
sound = sound or {}
for _, n in ipairs({ "AddSoundOverrides", "GetLoudestSoundHint", "SetActorGender" }) do if sound[n] == nil then sound[n] = stub("sound." .. n) end end
sql = sql or {}
for _, n in ipairs({ "Begin", "Commit", "IndexExists", "LastError", "Query", "QueryRow", "QueryTyped", "QueryValue", "SQLStr", "TableExists" }) do if sql[n] == nil then sql[n] = stub("sql." .. n) end end
steamworks = steamworks or {}
for _, n in ipairs({ "FileInfo", "GetList" }) do if steamworks[n] == nil then steamworks[n] = stub("steamworks." .. n) end end
usermessage = usermessage or {}
for _, n in ipairs({ "GetTable", "Hook", "IncomingMessage" }) do if usermessage[n] == nil then usermessage[n] = stub("usermessage." .. n) end end
widgets = widgets or {}
for _, n in ipairs({ "PlayerTick" }) do if widgets[n] == nil then widgets[n] = stub("widgets." .. n) end end

function DropEntityIfHeld(e) if IsValid(e) then e:ForcePlayerDrop() end end

function util.SHA256(s) return __sha256(tostring(s)) end
local TIMER = {}
TIMER.__index = TIMER
function TIMER:Start(dur) self.endtime = CurTime() + (dur or 0); self.started = CurTime() end
function TIMER:Elapsed() return CurTime() >= self.endtime end
function TIMER:Reset() self.endtime = 0 end
function TIMER:GetElapsedTime() return CurTime() - self.started end
function util.Timer(start) local t = setmetatable({ endtime = 0, started = CurTime() }, TIMER); if start then t:Start(start) end return t end

-- NPC capability flags (Enums/CAP)
CAP_MOVE_GROUND = 1
CAP_MOVE_JUMP = 2
CAP_MOVE_FLY = 4
CAP_MOVE_CLIMB = 8
CAP_SKIP_NAV_GROUND_CHECK = 16
CAP_MOVE_SHOOT = 32
CAP_USE = 64
CAP_AUTO_DOORS = 256
CAP_OPEN_DOORS = 512
CAP_TURN_HEAD = 1024
CAP_WEAPON_RANGE_ATTACK1 = 2048
CAP_WEAPON_RANGE_ATTACK2 = 4096
CAP_WEAPON_MELEE_ATTACK1 = 8192
CAP_WEAPON_MELEE_ATTACK2 = 16384
CAP_INNATE_RANGE_ATTACK1 = 32768
CAP_INNATE_RANGE_ATTACK2 = 65536
CAP_INNATE_MELEE_ATTACK1 = 131072
CAP_INNATE_MELEE_ATTACK2 = 262144
CAP_USE_WEAPONS = 524288
CAP_USE_SHOT_REGULATOR = 1048576
CAP_ANIMATEDFACE = 2097152
CAP_FRIENDLY_DMG_IMMUNE = 4194304
CAP_SQUAD = 8388608
CAP_DUCK = 16777216
CAP_NO_HIT_PLAYER = 33554432
CAP_AIM_GUN = 67108864
CAP_NO_HIT_SQUADMATES = 134217728
CAP_SIMPLE_RADIUS_DAMAGE = 268435456

-- player animation events / gesture slots (Enums/PLAYERANIMEVENT, Enums/GESTURE_SLOT)
PLAYERANIMEVENT_ATTACK_PRIMARY = 0
PLAYERANIMEVENT_ATTACK_SECONDARY = 1
PLAYERANIMEVENT_ATTACK_GRENADE = 2
PLAYERANIMEVENT_RELOAD = 3
PLAYERANIMEVENT_RELOAD_LOOP = 4
PLAYERANIMEVENT_RELOAD_END = 5
PLAYERANIMEVENT_JUMP = 6
PLAYERANIMEVENT_SWIM = 7
PLAYERANIMEVENT_DIE = 8
PLAYERANIMEVENT_FLINCH_CHEST = 9
PLAYERANIMEVENT_FLINCH_HEAD = 10
PLAYERANIMEVENT_FLINCH_LEFTARM = 11
PLAYERANIMEVENT_FLINCH_RIGHTARM = 12
PLAYERANIMEVENT_FLINCH_LEFTLEG = 13
PLAYERANIMEVENT_FLINCH_RIGHTLEG = 14
PLAYERANIMEVENT_DOUBLEJUMP = 15
PLAYERANIMEVENT_CANCEL = 16
PLAYERANIMEVENT_SPAWN = 17
PLAYERANIMEVENT_SNAP_YAW = 18
PLAYERANIMEVENT_CUSTOM = 19
PLAYERANIMEVENT_CUSTOM_GESTURE = 20
PLAYERANIMEVENT_CUSTOM_SEQUENCE = 21
PLAYERANIMEVENT_CUSTOM_GESTURE_SEQUENCE = 22
PLAYERANIMEVENT_CANCEL_RELOAD = 23
GESTURE_SLOT_ATTACK_AND_RELOAD = 0
GESTURE_SLOT_GRENADE = 1
GESTURE_SLOT_JUMP = 2
GESTURE_SLOT_SWIM = 3
GESTURE_SLOT_FLINCH = 4
GESTURE_SLOT_VCD = 5
GESTURE_SLOT_CUSTOM = 6

SCREENFADE_IN, SCREENFADE_OUT, SCREENFADE_MODULATE, SCREENFADE_STAYOUT, SCREENFADE_PURGE = 1, 2, 4, 8, 16
TRANSMIT_ALWAYS, TRANSMIT_NEVER, TRANSMIT_PVS = 0, 1, 2
STEPSOUNDTIME_NORMAL, STEPSOUNDTIME_ON_LADDER, STEPSOUNDTIME_WATER_KNEE, STEPSOUNDTIME_WATER_FOOT = 0, 1, 2, 3
SIM_NOTHING, SIM_LOCAL_ACCELERATION, SIM_LOCAL_FORCE, SIM_GLOBAL_ACCELERATION, SIM_GLOBAL_FORCE = 0, 1, 2, 3, 4
