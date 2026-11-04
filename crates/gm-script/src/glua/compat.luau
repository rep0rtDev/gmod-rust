-- The rest of the documented surface that needs no host object of its own: debug / jit over what
-- Luau exposes, coroutine.wait, usermessage / gameevent over the existing bridges, ISave /
-- IRestore, SurfaceInfo, IMaterial / ITexture from the mounted VMT / VTF files, the leftover
-- globals. Loaded last, so it sees both realms' tables.
local function ply_of(id) return Player and Player(id) or NULL end

------------------------------------------------------------------------ debug (Luau has debug.info / traceback; no hooks, locals or upvalue access)
debug = debug or {}
local dinfo, dtrace = __debug_info, __debug_traceback
debug.info = debug.info or dinfo
local WHAT = { Lua = "Lua", C = "C" }
function debug.getinfo(thread, f, what)
    if type(thread) ~= "thread" then thread, f, what = nil, thread, f end
    local level = type(f) == "number" and f + 1 or f
    if type(level) == "number" and level < 1 then return nil end
    if type(level) ~= "number" and type(level) ~= "function" then return nil end
    if not dinfo then return nil end
    local src, line, name, nparams, isvararg, fn = dinfo(level, "slnaf")
    if src == nil then return nil end
    local short = tostring(src):gsub("^[=@]", "")
    local short_src = short:match("([^/\\]+)$") or short
    return {
        source = "@" .. short, short_src = short_src, currentline = line or -1, linedefined = line or -1, lastlinedefined = -1,
        name = name, namewhat = name and "global" or "", what = (short == "[C]") and "C" or "Lua",
        nparams = nparams or 0, isvararg = isvararg == true, nups = 0, func = fn, activelines = {},
    }
end
function debug.traceback(thread, msg, level)
    if type(thread) ~= "thread" then thread, msg, level = nil, thread, msg end
    if msg ~= nil and type(msg) ~= "string" then return msg end
    local tb = dtrace and dtrace(level or 1) or "stack traceback:"
    if msg then return tostring(msg) .. "\n" .. tb end
    return tb
end
function debug.Trace()
    local out = { "Trace:" }
    for lvl = 2, 32 do
        if not dinfo then break end
        local src, line, name = dinfo(lvl, "sln")
        if src == nil then break end
        out[#out + 1] = string.format("  %d: Line %d \"%s\" %s", lvl - 1, line or -1, tostring(name or ""), tostring(src))
    end
    print(table.concat(out, "\n"))
end
function debug.getmetatable(v) return getmetatable(v) end
function debug.setmetatable(v, mt) setmetatable(v, mt); return v end
local registry = { _LOADED = {}, _G = _G }
function debug.getregistry() return registry end
function debug.getfenv(f) return (getfenv and getfenv(type(f) == "number" and f + 1 or f)) or _G end
function debug.setfenv(f, env) if setfenv then setfenv(type(f) == "number" and f + 1 or f, env) end return f end
local hook_fn, hook_mask, hook_count = nil, "", 0
function debug.sethook(thread, fn, mask, count) if type(thread) ~= "thread" then fn, mask, count = thread, fn, mask end hook_fn, hook_mask, hook_count = fn, mask or "", count or 0 end
function debug.gethook() return hook_fn, hook_mask, hook_count end
function debug.getlocal() return nil end
function debug.setlocal() return nil end
function debug.getupvalue() return nil end
function debug.setupvalue() return nil end
function debug.upvalueid(f, n) return tostring(f) .. ":" .. tostring(n) end
function debug.upvaluejoin() end
function debug.debug() end

------------------------------------------------------------------------ jit: Luau runs the actors (no trace JIT to switch)
jit = jit or {}
jit.arch = "x64"
jit.os = (package and package.config and package.config:sub(1, 1) == "\\") and "Windows" or "Linux"
jit.version = "Luau"
jit.version_num = 0
function jit.on() end
function jit.off() end
function jit.flush() end
function jit.attach() end
function jit.status() return false end

------------------------------------------------------------------------ coroutine.wait: yields until the game time passes
function coroutine.wait(seconds)
    local until_t = CurTime() + (tonumber(seconds) or 0)
    while CurTime() < until_t do coroutine.yield() end
end

numpad = numpad or {}
function numpad.FromButton() return __numpad_button end

------------------------------------------------------------------------ CSoundPatch / CUserCmd / CMoveData leftovers
do
    local SP = __glua_classes.CSoundPatch
    function SP:Stop() self.playing = false; if sound and sound.Stop then sound.Stop(self.name) end end
    function SP:FadeOut(sec) self:ChangeVolume(0, sec); timer.Simple(sec or 0, function() self.playing = false end) end
    local C = __glua_classes.CUserCmd
    function C:GetMouseWheel() return self.wheel or 0 end
    function C:SetMouseWheel(v) self.wheel = tonumber(v) or 0 end
    local M = __glua_classes.CMoveData
    function M:GetConstraintCenter() local c = self.constraint_center; return c and Vector(c.x, c.y, c.z) or Vector() end
    function M:SetConstraintCenter(v) self.constraint_center = { x = v.x, y = v.y, z = v.z } end
    function M:GetConstraintRadius() return self.constraint_radius or 0 end
    function M:SetConstraintRadius(r) self.constraint_radius = tonumber(r) or 0 end
    function M:GetConstraintWidth() return self.constraint_width or 0 end
    function M:SetConstraintWidth(w) self.constraint_width = tonumber(w) or 0 end
    function M:GetConstraintSpeedScale() return self.constraint_speed_scale or 0 end
    function M:SetConstraintSpeedScale(s) self.constraint_speed_scale = tonumber(s) or 0 end
    function M:GetAbsMoveAngles() local a = self.abs_angles or self.angles; return Angle(a.x, a.y, a.z) end
    function M:SetAbsMoveAngles(a) self.abs_angles = { x = a.p or a.x or 0, y = a.y or 0, z = a.r or a.z or 0 } end
end

------------------------------------------------------------------------ ISave / IRestore: the save blocks of ENTITY:OnSave / OnRestore (typed values in order)
do
    local SAVE = {}
    SAVE.__index = SAVE
    function __glua_isave(out) return setmetatable({ __out = out or {}, __blocks = {} }, SAVE) end
    local function w(self, kind, v) self.__out[#self.__out + 1] = { kind, v } end
    function SAVE:StartBlock(name) self.__blocks[#self.__blocks + 1] = name; w(self, "block", name) end
    function SAVE:EndBlock() self.__blocks[#self.__blocks] = nil; w(self, "end", true) end
    function SAVE:WriteAngle(a) w(self, "angle", { a.p, a.y, a.r }) end
    function SAVE:WriteBool(b) w(self, "bool", b == true) end
    function SAVE:WriteEntity(e) w(self, "entity", IsValid(e) and e:EntIndex() or 0) end
    function SAVE:WriteFloat(f) w(self, "float", tonumber(f) or 0) end
    function SAVE:WriteInt(i) w(self, "int", math.floor(tonumber(i) or 0)) end
    function SAVE:WriteString(s) w(self, "string", tostring(s)) end
    function SAVE:WriteVector(v) w(self, "vector", { v.x, v.y, v.z }) end
    __glua_classes.ISave = SAVE
    local RESTORE = {}
    RESTORE.__index = RESTORE
    function __glua_irestore(data) return setmetatable({ __in = data or {}, __p = 1 }, RESTORE) end
    local function r(self, kind)
        local e = self.__in[self.__p]
        if not e then return nil end
        self.__p = self.__p + 1
        return e[1] == kind and e[2] or nil
    end
    function RESTORE:StartBlock() local name = r(self, "block"); return name or "" end
    function RESTORE:EndBlock() r(self, "end") end
    function RESTORE:ReadAngle() local a = r(self, "angle") or { 0, 0, 0 }; return Angle(a[1], a[2], a[3]) end
    function RESTORE:ReadBool() return r(self, "bool") == true end
    function RESTORE:ReadEntity() return Entity(r(self, "entity") or 0) end
    function RESTORE:ReadFloat() return r(self, "float") or 0 end
    function RESTORE:ReadInt() return r(self, "int") or 0 end
    function RESTORE:ReadString() return r(self, "string") or "" end
    function RESTORE:ReadVector() local v = r(self, "vector") or { 0, 0, 0 }; return Vector(v[1], v[2], v[3]) end
    __glua_classes.IRestore = RESTORE
end

------------------------------------------------------------------------ SurfaceInfo: one brush face (material, winding, flags) as Entity:GetBrushSurfaces hands it out
do
    local SI = {}
    SI.__index = SI
    function __glua_surfaceinfo(t) return setmetatable({ __mat = t.material or "", __verts = t.verts or {}, __flags = t.flags or 0 }, SI) end
    function SI:GetMaterial() return Material(self.__mat) end
    function SI:GetVertices() local out = {}; for i, v in ipairs(self.__verts) do out[i] = Vector(v.x or v[1], v.y or v[2], v.z or v[3]) end return out end
    local SURF_SKY, SURF_NODRAW = 0x4, 0x80
    function SI:IsNoDraw() return bit.band(self.__flags, SURF_NODRAW) ~= 0 end
    function SI:IsSky() return bit.band(self.__flags, SURF_SKY) ~= 0 end
    function SI:IsWater() return string.find(string.lower(self.__mat), "water", 1, true) ~= nil end
    __glua_classes.SurfaceInfo = SI
end

------------------------------------------------------------------------ util leftovers
function util.SHA1(s) return __sha1(tostring(s)) end
function util.MD5(s) return __md5(tostring(s)) end
-- the shared animation events of the SDK's eventlist (AE_* ids the models' sequences carry)
local ANIM_EVENTS = {
    AE_EMPTY = 0, AE_NPC_LEFTFOOT = 2050, AE_NPC_RIGHTFOOT = 2051, AE_NPC_BODYDROP_LIGHT = 2001, AE_NPC_BODYDROP_HEAVY = 2002,
    AE_NPC_SWISHSOUND = 2010, AE_NPC_180TURN = 2020, AE_NPC_ITEM_PICKUP = 2040, AE_NPC_WEAPON_DROP = 2041,
    AE_NPC_WEAPON_SET_SEQUENCE_NAME = 2042, AE_NPC_WEAPON_SET_SEQUENCE_NUMBER = 2043, AE_NPC_WEAPON_SET_ACTIVITY = 2044,
    AE_NPC_HOLSTER = 2045, AE_NPC_DRAW = 2046, AE_NPC_WEAPON_FIRE = 3001, AE_NPC_ATTACK_BROADCAST = 2060,
    AE_NPC_HURT_INTERACTION_PARTNER = 2061, AE_NPC_SET_INTERACTION_CANTDIE = 2062, AE_NPC_RAGDOLL = 2070,
    AE_NPC_ADDGESTURE = 2080, AE_NPC_RESTARTGESTURE = 2081, AE_SV_PLAYSOUND = 5005, AE_CL_PLAYSOUND = 5004,
    AE_CL_STOPSOUND = 5006, AE_START_SCRIPTED_EFFECT = 5007, AE_STOP_SCRIPTED_EFFECT = 5008, AE_CLIENT_EFFECT_ATTACH = 5009,
    AE_MUZZLEFLASH = 5001, AE_NPC_MUZZLEFLASH = 5002, AE_THUMPER_THUMP = 5003, AE_RAGDOLL = 6001,
    AE_CL_CREATE_PARTICLE_EFFECT = 6002, AE_CL_STOP_PARTICLE_EFFECT = 6003, AE_CL_ADD_PARTICLE_EFFECT_CP = 6004,
    AE_CL_CREATE_PARTICLE_BRASS = 6005, AE_CL_ENABLE_BODYGROUP = 6006, AE_CL_DISABLE_BODYGROUP = 6007,
    AE_CL_BODYGROUP_SET_VALUE = 6008, AE_CL_BODYGROUP_SET_VALUE_CMODEL_WPN = 6009, AE_WPN_PRIMARYATTACK = 7001,
    AE_WPN_INCREMENTAMMO = 7002, AE_WPN_HIDE = 7003, AE_WPN_UNHIDE = 7004, AE_WPN_PLAYWPNSOUND = 7005,
}
local ANIM_EVENT_NAMES = {}
for k, v in pairs(ANIM_EVENTS) do ANIM_EVENT_NAMES[v] = k; _G[k] = _G[k] or v end
function util.GetAnimEventIDByName(name) return ANIM_EVENTS[tostring(name)] or -1 end
function util.GetAnimEventNameByID(id) return ANIM_EVENT_NAMES[tonumber(id) or -1] or "" end
-- settings/users.txt as the game ships it (groups with their members by SteamID)
local function parse_users()
    local groups = { superadmin = {}, admin = {}, user = {} }
    local text = file and file.Read and file.Read("settings/users.txt", "GAME") or (__content_read and __content_read("settings/users.txt"))
    if not text then return groups end
    local kv = util.KeyValuesToTable and util.KeyValuesToTable(text, false, true)
    if istable(kv) then for g, members in pairs(kv) do if istable(members) then groups[g] = members end end end
    return groups
end
function util.GetUserGroups()
    local g = parse_users()
    local out = {}
    for name in pairs(g) do out[name] = { name = name, members = g[name] } end
    return out
end

------------------------------------------------------------------------ game / physenv / player leftovers
function game.GetAmmoMax(id) local d = game.GetAmmoData(id); return d and d.maxcarry or -1 end
local map_changes = 0
function game.GetMapChangeCount() return map_changes end
function game.MapLoadType() return "newgame" end
function game.GetWindSpeed()
    local w = ents and ents.FindByClass and ents.FindByClass("env_wind")[1]
    if not IsValid(w) then return Vector() end
    local kv = w.GetKeyValues and w:GetKeyValues() or {}
    local yaw = tonumber(kv.angles and tostring(kv.angles):match("[-%d.]+ ([-%d.]+)") or 0) or 0
    local speed = tonumber(kv.minwind or 0) or 0
    return Angle(0, yaw, 0):Forward() * speed
end
function game.Get3DSkyboxInfo()
    local cam = ents and ents.FindByClass and ents.FindByClass("sky_camera")[1]
    if not IsValid(cam) then return Vector(), 16 end
    local kv = cam.GetKeyValues and cam:GetKeyValues() or {}
    return cam:GetPos(), tonumber(kv.scale) or 16
end
local perf = { MaxVelocity = 4000, MaxAngularVelocity = 7200, MaxCollisionsPerObjectPerTimestep = 6, MaxCollisionChecksPerTimestep = 50000,
    LookAheadTimeObjectsVsWorld = 1, LookAheadTimeObjectsVsObject = 0.5, MinFrictionMass = 10, MaxFrictionMass = 2500 }
function physenv.GetPerformanceSettings() return table.Copy(perf) end
function physenv.SetPerformanceSettings(t) for k, v in pairs(t or {}) do if perf[k] ~= nil then perf[k] = tonumber(v) or perf[k] end end
    if t and t.MaxVelocity and RunConsoleCommand then RunConsoleCommand("sv_maxvelocity", tostring(t.MaxVelocity)) end
end
local air_density = 2
function physenv.GetAirDensity() return air_density end
function physenv.SetAirDensity(d) air_density = tonumber(d) or air_density end
local extra_surfaces = {}
function physenv.AddSurfaceData(text)
    local kv = util.KeyValuesToTable and util.KeyValuesToTable("\"surfaceprops\" {" .. tostring(text) .. "}", false, true) or {}
    for name, props in pairs(kv) do if istable(props) then extra_surfaces[string.lower(name)] = props end end
end
function physenv.__surface(name) return extra_surfaces[string.lower(tostring(name))] end
function player.GetCountConnecting() return 0 end

------------------------------------------------------------------------ resource: the server's download list (what every client fetches with the addon)
resource = resource or {}
local resources, workshop_ids = {}, {}
local function add_file(path, single)
    path = tostring(path)
    if resources[path] then return end
    resources[path] = true
    if single then return end
    local base = path:gsub("%.mdl$", "")
    if base ~= path then for _, ext in ipairs({ ".vvd", ".dx90.vtx", ".phy", ".ani" }) do resources[base .. ext] = true end end
    local vmt = path:gsub("%.vmt$", "")
    if vmt ~= path and __content_read then
        local text = __content_read("materials/" .. path:gsub("^materials/", ""))
        for tex in tostring(text or ""):gmatch('"?%$%w*texture%d*"?%s+"?([^"\n]+)"?') do resources["materials/" .. tex:gsub("%.vtf$", "") .. ".vtf"] = true end
    end
end
function resource.AddFile(path) add_file(path, false) end
function resource.AddSingleFile(path) add_file(path, true) end
function resource.AddWorkshop(id) workshop_ids[#workshop_ids + 1] = tostring(id) end
function resource.__files() local out = {}; for p in pairs(resources) do out[#out + 1] = p end table.sort(out); return out end
function resource.__workshop() return workshop_ids end

------------------------------------------------------------------------ gameevent.Listen: the engine's game events as tables through hook.Run(eventname, data)
gameevent = gameevent or {}
local listened = {}
local GAME_EVENTS = {
    player_connect = { "PlayerConnect", function(name) return { name = name, userid = 0, networkid = "", address = "", bot = 0, index = 0 } end },
    player_activate = { "PlayerInitialSpawn", function(ply) return { userid = ply:UserID() } end },
    player_spawn = { "PlayerSpawn", function(ply) return { userid = ply:UserID() } end },
    player_disconnect = { "PlayerDisconnected", function(ply) return { userid = ply:UserID(), name = ply:Nick(), networkid = ply:SteamID(), reason = "Disconnect", bot = ply:IsBot() and 1 or 0 } end },
    player_hurt = { "PlayerHurt", function(ply, attacker, remaining, dmg) return { userid = ply:UserID(), attacker = IsValid(attacker) and attacker.UserID and attacker:UserID() or 0, health = remaining, priority = 5 } end },
    player_death = { "PlayerDeath", function(ply, inflictor, attacker) return { userid = ply:UserID(), attacker = IsValid(attacker) and attacker.UserID and attacker:UserID() or 0, entindex_inflictor = IsValid(inflictor) and inflictor:EntIndex() or 0, entindex_attacker = IsValid(attacker) and attacker:EntIndex() or 0, weapon = IsValid(inflictor) and inflictor:GetClass() or "", priority = 7 } end },
    entity_killed = { "OnNPCKilled", function(npc, attacker, inflictor) return { entindex_killed = npc:EntIndex(), entindex_attacker = IsValid(attacker) and attacker:EntIndex() or 0, entindex_inflictor = IsValid(inflictor) and inflictor:EntIndex() or 0, damagebits = 0 } end },
    player_say = { "PlayerSay", function(ply, text) return { userid = ply:UserID(), text = text, priority = 1 } end },
    break_prop = { "PropBreak", function(ply, prop) return { userid = IsValid(ply) and ply:UserID() or 0, entindex = prop:EntIndex() } end },
    player_changename = { "PlayerChangedName", function(ply, old, new) return { userid = ply:UserID(), oldname = old, newname = new } end },
    server_cvar = { "__server_cvar", function(name, value) return { cvarname = name, cvarvalue = value } end },
    player_info = { "PlayerAuthed", function(ply, sid) return { name = ply:Nick(), index = ply:EntIndex() - 1, userid = ply:UserID(), networkid = sid, bot = ply:IsBot() and 1 or 0, friendsid = 0 } end },
    OnRequestFullUpdate = { "PlayerInitialSpawn", function(ply) return { userid = ply:UserID(), networkid = ply:SteamID(), name = ply:Nick(), index = ply:EntIndex() - 1 } end },
}
function gameevent.Listen(name)
    name = tostring(name)
    if listened[name] then return end
    listened[name] = true
    local ev = GAME_EVENTS[name]
    if not ev then return end
    hook.Add(ev[1], "__gameevent_" .. name, function(...) hook.Run(name, ev[2](...)) end)
end
function gameevent.__listened() return listened end

------------------------------------------------------------------------ usermessage: the receiving side of umsg (client), over the bf_read reader
usermessage = usermessage or {}
local um_hooks = {}
function usermessage.Hook(name, fn, ...) um_hooks[tostring(name)] = { fn = fn, args = { ... } } end
function usermessage.GetTable() return um_hooks end
function usermessage.IncomingMessage(name, msg)
    local h = um_hooks[tostring(name)]
    if not h then return end
    if type(msg) == "string" then msg = __glua_bf_read(msg) end
    h.fn(msg, unpack(h.args))
end

------------------------------------------------------------------------ HTTP: one blocking request per call, the callbacks run with the reply
local function http_request(p)
    local method = string.upper(tostring(p.method or "GET"))
    local url = tostring(p.url or "")
    local body = p.body
    local headers = {}
    for k, v in pairs(p.headers or {}) do headers[tostring(k)] = tostring(v) end
    if p.parameters and next(p.parameters) then
        local parts = {}
        for k, v in pairs(p.parameters) do parts[#parts + 1] = tostring(k) .. "=" .. tostring(v):gsub("[^%w%-%._~]", function(c) return string.format("%%%02X", string.byte(c)) end) end
        local q = table.concat(parts, "&")
        if method == "GET" or method == "HEAD" then url = url .. (url:find("?", 1, true) and "&" or "?") .. q
        elseif body == nil then body = q; headers["Content-Type"] = headers["Content-Type"] or "application/x-www-form-urlencoded" end
    end
    if p.type and body ~= nil then headers["Content-Type"] = tostring(p.type) end
    local code, out, rh = __http(method, url, body, headers)
    if code == nil then
        if p.failed then p.failed(tostring(out)) end
        return false
    end
    if p.success then p.success(code, out, rh or {}) end
    return true
end
function HTTP(p) return http_request(p) end
http = http or {}
function http.Fetch(url, onSuccess, onFailure, headers)
    return http_request({ url = url, method = "GET", headers = headers,
        success = function(code, body, h) if onSuccess then onSuccess(body, #body, h, code) end end, failed = onFailure })
end
function http.Post(url, parameters, onSuccess, onFailure, headers)
    return http_request({ url = url, method = "POST", parameters = parameters, headers = headers,
        success = function(code, body, h) if onSuccess then onSuccess(body, #body, h, code) end end, failed = onFailure })
end

------------------------------------------------------------------------ leftover globals
function DEFINE_BASECLASS(name) return baseclass and baseclass.Get and baseclass.Get(name) or nil end
function IsFirstTimePredicted() return true end
local prediction_player = nil
function GetPredictionPlayer() return prediction_player or (CLIENT and LocalPlayer and LocalPlayer()) or NULL end
function __glua_set_prediction_player(ply) prediction_player = ply end
function PrecacheScene(scene) return true end
function PrecacheSentenceFile(file) return true end
function PrecacheSentenceGroup(group) return true end
function PrecacheParticleSystem(name) return true end
local constraint_system = nil
function SetPhysConstraintSystem(ent) constraint_system = ent end
function GetPhysConstraintSystem() return constraint_system end
local suppressed_host = nil
function SuppressHostEvents(ply) suppressed_host = ply end
function GetSuppressedHostEvents() return suppressed_host end
local nw_var_tables = {}
function BuildNetworkedVarsTable()
    local out = {}
    for _, e in ipairs(ents.GetAll()) do local nw = e.GetNWVars and e:GetNWVars(); if nw and next(nw) then out[e:EntIndex()] = nw end end
    return out
end
function DTVar_ReceiveProxyGL(ent, name, old, new) if ent and ent.NetworkVarNotify then ent:NetworkVarNotify(name, old, new) end return new end
local debug_slots = {}
function DebugInfo(slot, text) debug_slots[tonumber(slot) or 0] = tostring(text); if world and world.queue then pcall(world.queue, "debug.info", 0, { slot = slot, text = tostring(text) }) end end
local pvs_origins = {}
function AddOriginToPVS(pos) pvs_origins[#pvs_origins + 1] = { x = pos.x, y = pos.y, z = pos.z } end
function __glua_pvs_origins() local t = pvs_origins; pvs_origins = {}; return t end
function CompileFile(path, identifier)
    path = tostring(path)
    local src = (file and file.Read and (file.Read(path, "LUA") or file.Read("lua/" .. path, "GAME"))) or (__content_read and (__content_read("lua/" .. path) or __content_read(path)))
    if not src then return nil end
    local fn, err = __compile(src, identifier or path)
    if not fn then ErrorNoHalt(tostring(err) .. "\n"); return nil end
    return fn
end
function BroadcastLua(code)
    if net and net.Start and util.AddNetworkString then
        util.AddNetworkString("__gm_broadcast_lua")
        net.Start("__gm_broadcast_lua"); net.WriteString(tostring(code)); net.Broadcast()
    end
end
if CLIENT and net and net.Receive then
    net.Receive("__gm_broadcast_lua", function() RunString(net.ReadString(), "BroadcastLua") end)
end

-- sentences: scripts/sentences.txt groups (`name  wave,wave,...` lines) -> the first wave of the
-- named sentence, numbered members of a group round-robin
local sentences = nil
local function load_sentences()
    if sentences then return sentences end
    sentences = {}
    local text = __content_read and __content_read("scripts/sentences.txt")
    for line in tostring(text or ""):gmatch("[^\r\n]+") do
        line = line:gsub("//.*$", "")
        local name, rest = line:match("^%s*(%S+)%s+(.-)%s*$")
        if name and rest and rest ~= "" then
            local folder, waves = rest:match("^(%S+/)%s*(.*)$")
            if not folder then folder, waves = "", rest end
            local list = {}
            for w in waves:gmatch("%S+") do
                local file = w:gsub("%(.-%)", ""):gsub("[,.]+$", "")
                if file ~= "" then list[#list + 1] = folder .. file .. ".wav" end
            end
            sentences[string.upper(name)] = list
        end
    end
    return sentences
end
local function sentence_waves(name)
    name = string.upper(tostring(name)):gsub("^!", "")
    local s = load_sentences()
    if s[name] then return s[name] end
    local group = {}
    for k, v in pairs(s) do if k:match("^" .. name:gsub("%p", "%%%0") .. "%d+$") then group[#group + 1] = v end end
    if #group == 0 then return nil end
    return group[math.random(#group)]
end
function SentenceDuration(name)
    local waves = sentence_waves(name)
    if not waves then return 0 end
    local total = 0
    for _, w in ipairs(waves) do total = total + (SoundDuration(w) or 0) end
    return total
end
function EmitSentence(name, pos, entity, channel, volume, level, flags, pitch)
    local waves = sentence_waves(name)
    if not waves then return end
    local p = pos or (IsValid(entity) and entity:GetPos()) or Vector()
    local at = 0
    for _, w in ipairs(waves) do
        if at == 0 then sound.Play(w, p, level or 75, pitch or 100, volume or 1)
        else timer.Simple(at, function() sound.Play(w, p, level or 75, pitch or 100, volume or 1) end) end
        at = at + (SoundDuration(w) or 0)
    end
end

-- particles: a particle system by name is a client effect; the server sends it, the client realm
-- raises the effect for its own particle code (`Particle` / `ParticleEffect*` hooks)
local function particle_send(t)
    if SERVER and net and net.Start and util.AddNetworkString then
        util.AddNetworkString("__gm_particle")
        net.Start("__gm_particle"); net.WriteTable(t); net.Broadcast()
    elseif CLIENT then hook.Run("__ParticleEffect", t) end
end
function ParticleEffect(name, pos, angles, parent)
    particle_send({ name = tostring(name), pos = { pos.x, pos.y, pos.z }, ang = { angles and angles.p or 0, angles and angles.y or 0, angles and angles.r or 0 }, parent = IsValid(parent) and parent:EntIndex() or 0 })
end
function ParticleEffectAttach(name, attachType, entity, attachmentID)
    particle_send({ name = tostring(name), attach = tonumber(attachType) or 0, parent = IsValid(entity) and entity:EntIndex() or 0, attachment = tonumber(attachmentID) or 0 })
end
function Particle(name, pos, angles, parent) return ParticleEffect(name, pos, angles, parent) end
if CLIENT and net and net.Receive then net.Receive("__gm_particle", function() hook.Run("__ParticleEffect", net.ReadTable()) end) end

-- TauntCamera: the third person camera of a taunt (the game's object for CalcView / ShouldDrawLocalPlayer)
local TAUNT = {}
TAUNT.__index = TAUNT
function TauntCamera() return setmetatable({ enabled = false, dist = 100, start = 0, last_angles = Angle() }, TAUNT) end
function TAUNT:CalcView(view, ply, origin, angles, fov)
    if not IsValid(ply) or not ply:IsPlayingTaunt() then
        if self.enabled then self.enabled = false end
        return false
    end
    if not self.enabled then self.enabled = true; self.start = CurTime(); self.last_angles = Angle(angles.p, angles.y, angles.r) end
    local ang = self.last_angles
    local dist = math.min(self.dist, 100)
    local tr = util.TraceHull({ start = origin, endpos = origin - ang:Forward() * dist, filter = ply, mins = Vector(-8, -8, -8), maxs = Vector(8, 8, 8) })
    view.origin = tr.HitPos
    view.angles = ang
    view.drawviewer = true
    return true
end
function TAUNT:ShouldDrawLocalPlayer(ply) return self.enabled end
function TAUNT:ShouldDrawThirdPerson(ply) return self.enabled end

------------------------------------------------------------------------ IMaterial / ITexture: the mounted VMT / VTF files (parameters, shader, texture sizes)
do
    -- VMT: `"Shader" { "$key" "value" ... "Proxies" { ... } }`, `//` comments, unquoted tokens
    local function vmt_parse(text)
        local toks, i, n = {}, 1, #text
        while i <= n do
            local c = text:sub(i, i)
            if c:match("%s") then i = i + 1
            elseif c == "/" and text:sub(i + 1, i + 1) == "/" then i = (text:find("\n", i, true) or n) + 1
            elseif c == "{" or c == "}" then toks[#toks + 1] = c; i = i + 1
            elseif c == '"' then local j = text:find('"', i + 1, true) or n; toks[#toks + 1] = text:sub(i + 1, j - 1); i = j + 1
            else local j = text:find("[%s{}]", i) or n + 1; toks[#toks + 1] = text:sub(i, j - 1); i = j end
        end
        local p = 1
        local function block()
            local t = {}
            while p <= #toks do
                local k = toks[p]
                if k == "}" then p = p + 1; return t end
                p = p + 1
                if toks[p] == "{" then p = p + 1; t[k] = block() else t[k] = toks[p]; p = p + 1 end
            end
            return t
        end
        local shader = toks[1] or "VertexLitGeneric"
        p = 3
        local params = block()
        local lower = {}
        for k, v in pairs(params) do lower[string.lower(k)] = v end
        return shader, lower
    end
    local vtf_cache = {}
    local function vtf_header(name)
        name = string.lower(tostring(name or "")):gsub("\\", "/"):gsub("^materials/", ""):gsub("%.vtf$", "")
        if vtf_cache[name] ~= nil then return vtf_cache[name] or nil end
        local data = __content_read and __content_read("materials/" .. name .. ".vtf")
        if not data or #data < 32 or data:sub(1, 4) ~= "VTF\0" then vtf_cache[name] = false; return nil end
        local function u16(o) local a, b = data:byte(o + 1, o + 2); return a + b * 256 end
        local h = { width = u16(16), height = u16(18), frames = u16(24), found = true }
        vtf_cache[name] = h
        return h
    end

    local MAT = __glua_classes.IMaterial or {}
    MAT.__index = MAT
    __glua_classes.IMaterial = MAT
    local mats = {}
    local function load(self)
        if self.__loaded then return end
        self.__loaded = true
        self.__kv = self.__kv or {}
        self.__shader = self.__shader or "VertexLitGeneric"
        local text = __content_read and self.__name ~= "" and __content_read("materials/" .. self.__name:gsub("%.vmt$", "") .. ".vmt")
        if text then
            local shader, params = vmt_parse(text)
            self.__shader, self.__found = shader, true
            for k, v in pairs(params) do if self.__kv[k] == nil then self.__kv[k] = v end end
        elseif self.__params and self.__params ~= "" then
            self.__shader, self.__found = "UnlitGeneric", true
            self.__kv["$basetexture"] = self.__kv["$basetexture"] or self.__name:gsub("%.vtf$", ""):gsub("%.png$", "")
            for flag in tostring(self.__params):gmatch("%S+") do self.__kv["$" .. flag] = "1" end
        else
            self.__found = __content_read == nil
        end
    end
    function Material(name, params)
        name = tostring(name or ""):gsub("\\", "/")
        local key = name .. "|" .. tostring(params or "")
        local m = mats[key]
        if m then return m, 0 end
        local t0 = SysTime and SysTime() or 0
        m = setmetatable({ __name = name, __params = params or "", __kv = {} }, MAT)
        load(m)
        mats[key] = m
        return m, (SysTime and SysTime() or 0) - t0
    end
    function CreateMaterial(name, shader, data)
        local m = setmetatable({ __name = tostring(name), __params = "", __kv = {}, __shader = tostring(shader or "UnlitGeneric"), __loaded = true, __found = true }, MAT)
        for k, v in pairs(data or {}) do m.__kv[string.lower(tostring(k))] = v end
        m.__data = data or {}
        mats[m.__name .. "|"] = m
        return m
    end
    local function key(k) return string.lower(tostring(k or "")) end
    function MAT:GetName() return self.__name end
    function MAT:IsError() load(self); return self.__name == "" or self.__found == false end
    function MAT:GetShader() load(self); return self.__shader end
    function MAT:SetShader(s) self.__shader = tostring(s) end
    function MAT:GetKeyValues() load(self); return table.Copy(self.__kv) end
    function MAT:GetString(k) load(self); local v = self.__kv[key(k)]; if v == nil then return nil end return type(v) == "table" and tostring(v) or tostring(v) end
    function MAT:GetInt(k) local v = self:GetString(k); return v and math.floor(tonumber(v) or 0) or nil end
    function MAT:GetFloat(k) local v = self:GetString(k); return v and (tonumber(v) or 0) or nil end
    local function nums(s) local out = {}; for x in tostring(s):gmatch("[-%d.eE]+") do out[#out + 1] = tonumber(x) or 0 end return out end
    function MAT:GetVector(k)
        load(self); local v = self.__kv[key(k)]
        if v == nil then return nil end
        if type(v) == "table" and v.x then return Vector(v.x, v.y, v.z) end
        local s = tostring(v); local a = nums(s)
        if s:sub(1, 1) == "{" then return Vector((a[1] or 0) / 255, (a[2] or 0) / 255, (a[3] or 0) / 255) end
        return Vector(a[1] or 0, a[2] or a[1] or 0, a[3] or a[1] or 0)
    end
    function MAT:GetVectorLinear(k) local v = self:GetVector(k); if not v then return nil end return Vector(v.x ^ 2.2, v.y ^ 2.2, v.z ^ 2.2) end
    function MAT:GetVector4D(k)
        load(self); local v = self.__kv[key(k)]
        if v == nil then return nil end
        local a = nums(v); return a[1] or 0, a[2] or 0, a[3] or 0, a[4] or 1
    end
    function MAT:GetMatrix(k)
        load(self); local v = self.__kv[key(k)]
        if v == nil then return nil end
        if type(v) == "table" and v.GetTranslation then return v end
        local m = Matrix()
        local s = tostring(v)
        local cx, cy = s:match("center%s+([-%d.]+)%s+([-%d.]+)")
        local sx, sy = s:match("scale%s+([-%d.]+)%s+([-%d.]+)")
        local rot = s:match("rotate%s+([-%d.]+)")
        local tx, ty = s:match("translate%s+([-%d.]+)%s+([-%d.]+)")
        if not (cx or sx or rot or tx) then local a = nums(s); if #a >= 16 then m:SetUnpacked(unpack(a)) end return m end
        m:Translate(Vector(tonumber(cx) or 0.5, tonumber(cy) or 0.5, 0))
        m:Scale(Vector(tonumber(sx) or 1, tonumber(sy) or 1, 1))
        m:Rotate(Angle(0, tonumber(rot) or 0, 0))
        m:Translate(Vector(-(tonumber(cx) or 0.5) + (tonumber(tx) or 0), -(tonumber(cy) or 0.5) + (tonumber(ty) or 0), 0))
        return m
    end
    local TEX = {}
    TEX.__index = TEX
    __glua_classes.ITexture = TEX
    local texs = {}
    local function texture(name)
        name = tostring(name)
        if texs[name] then return texs[name] end
        local t = setmetatable({ __name = name }, TEX)
        texs[name] = t
        return t
    end
    __glua_texture = texture
    function MAT:GetTexture(k)
        load(self); k = key(k)
        if k == "$basetexture" and self.__dynamic then return self.__dynamic end
        local v = self.__kv[k]
        if v == nil then return nil end
        if type(v) == "table" and v.__name then return v end
        return texture(v)
    end
    function MAT:SetDynamicImage(tex) self.__dynamic = tex end
    function MAT:Width() local t = self:GetTexture("$basetexture"); return t and t:Width() or 0 end
    function MAT:Height() local t = self:GetTexture("$basetexture"); return t and t:Height() or 0 end
    function MAT:GetColor(x, y) local t = self:GetTexture("$basetexture"); return t and t:GetColor(x, y) or Color(255, 255, 255, 255) end
    function MAT:SetFloat(k, v) load(self); self.__kv[key(k)] = tostring(tonumber(v) or 0) end
    function MAT:SetInt(k, v) load(self); self.__kv[key(k)] = tostring(math.floor(tonumber(v) or 0)) end
    function MAT:SetString(k, v) load(self); self.__kv[key(k)] = tostring(v) end
    function MAT:SetVector(k, v) load(self); self.__kv[key(k)] = string.format("[%s %s %s]", tostring(v.x), tostring(v.y), tostring(v.z)) end
    function MAT:SetVector4D(k, x, y, z, w) load(self); self.__kv[key(k)] = string.format("[%s %s %s %s]", tostring(x), tostring(y), tostring(z), tostring(w)) end
    function MAT:SetMatrix(k, m) load(self); self.__kv[key(k)] = m end
    function MAT:SetTexture(k, t) load(self); self.__kv[key(k)] = type(t) == "table" and t or texture(tostring(t)) end
    function MAT:SetUndefined(k) load(self); self.__kv[key(k)] = nil end
    function MAT:Recompute() end
    function MAT:IsValid() return true end
    function TEX:GetName() return self.__name end
    function TEX:Width() local h = vtf_header(self.__name); return h and h.width or 0 end
    function TEX:Height() local h = vtf_header(self.__name); return h and h.height or 0 end
    function TEX:GetMappingWidth() return self:Width() end
    function TEX:GetMappingHeight() return self:Height() end
    function TEX:GetNumAnimationFrames() local h = vtf_header(self.__name); return h and h.frames or 1 end
    function TEX:IsError() return __content_read ~= nil and vtf_header(self.__name) == nil end
    function TEX:IsErrorTexture() return self:IsError() end
    function TEX:Download() end
    -- pixel reads need the decoded image: the client's render target reads land here through
    -- `__texture_color` when the renderer has the texture, otherwise the texture's average is white
    function TEX:GetColor(x, y)
        if __texture_color then local r, g, b, a = __texture_color(self.__name, x, y); if r then return Color(r, g, b, a) end end
        return Color(255, 255, 255, 255)
    end
    function TEX:IsValid() return true end
end

------------------------------------------------------------------------ sql: the sv.db / cl.db of the game as a small SQL interpreter over the actor's storage
-- (CREATE / DROP TABLE and INDEX, INSERT [OR REPLACE | OR IGNORE], SELECT with WHERE / ORDER BY
-- / LIMIT and the common functions, UPDATE, DELETE; every value comes back as a string like
-- sqlite's text results, QueryTyped keeps numbers)
sql = sql or {}
do
    local DB_FILE = (CLIENT or MENU_DLL) and "cl.db.json" or "sv.db.json"
    local db, dirty, in_tx, last_error = nil, false, false, nil
    local function load_db()
        if db then return db end
        local text = file and file.Read and file.Read(DB_FILE, "DATA")
        db = text and util.JSONToTable(text) or nil
        if type(db) ~= "table" then db = { tables = {}, indexes = {} } end
        db.tables = db.tables or {}; db.indexes = db.indexes or {}
        return db
    end
    local function save()
        if in_tx then dirty = true; return end
        if file and file.Write then file.Write(DB_FILE, util.TableToJSON(load_db())) end
        dirty = false
    end

    -- tokens: words, numbers, 'strings', punctuation
    local function lex(q)
        local toks, i, n = {}, 1, #q
        while i <= n do
            local c = q:sub(i, i)
            if c:match("%s") then i = i + 1
            elseif c == "'" then
                local buf, j = {}, i + 1
                while j <= n do
                    local d = q:sub(j, j)
                    if d == "'" then if q:sub(j + 1, j + 1) == "'" then buf[#buf + 1] = "'"; j = j + 2 else break end
                    else buf[#buf + 1] = d; j = j + 1 end
                end
                toks[#toks + 1] = { t = "str", v = table.concat(buf) }; i = j + 1
            elseif c == '"' or c == "`" or c == "[" then
                local close = c == "[" and "]" or c
                local j = q:find(close, i + 1, true) or n
                toks[#toks + 1] = { t = "id", v = q:sub(i + 1, j - 1) }; i = j + 1
            elseif c:match("[%d]") or (c == "." and q:sub(i + 1, i + 1):match("%d")) then
                local j = i; while j <= n and q:sub(j, j):match("[%d%.eE]") do j = j + 1 end
                toks[#toks + 1] = { t = "num", v = tonumber(q:sub(i, j - 1)) or 0 }; i = j
            elseif c:match("[%a_]") then
                local j = i; while j <= n and q:sub(j, j):match("[%w_%.]") do j = j + 1 end
                toks[#toks + 1] = { t = "word", v = q:sub(i, j - 1) }; i = j
            else
                local two = q:sub(i, i + 1)
                if two == "<=" or two == ">=" or two == "<>" or two == "!=" or two == "==" or two == "||" then toks[#toks + 1] = { t = "op", v = two }; i = i + 2
                else toks[#toks + 1] = { t = "op", v = c }; i = i + 1 end
            end
        end
        return toks
    end

    local P = {}
    P.__index = P
    local function parser(toks) return setmetatable({ toks = toks, p = 1 }, P) end
    function P:peek(k) local t = self.toks[self.p + (k or 0)]; return t end
    function P:next() local t = self.toks[self.p]; self.p = self.p + 1; return t end
    function P:is(v) local t = self:peek(); return t and (t.t == "word" or t.t == "op") and string.upper(t.v) == v end
    function P:accept(v) if self:is(v) then self.p = self.p + 1; return true end return false end
    function P:expect(v) if not self:accept(v) then error("near \"" .. tostring(self:peek() and self:peek().v or "end") .. "\": syntax error", 0) end end
    function P:ident() local t = self:next(); if not t or (t.t ~= "word" and t.t ~= "id") then error("syntax error: expected a name", 0) end return t.v end

    local function like(s, pat)
        pat = tostring(pat):gsub("[%^%$%(%)%%%.%[%]%+%-%?]", "%%%0"):gsub("%%%%", "\0"):gsub("%%", ".*"):gsub("_", "."):gsub("%z", "%%%%")
        return string.find(string.lower(tostring(s)), "^" .. string.lower(pat) .. "$") ~= nil
    end
    local function num(v) return tonumber(v) end
    local function truthy(v) if v == nil then return false end local n = num(v); if n then return n ~= 0 end return v ~= "" end
    local function cmp(a, b)
        if a == nil or b == nil then return nil end
        local na, nb = num(a), num(b)
        if na and nb then return na < nb and -1 or (na > nb and 1 or 0) end
        a, b = tostring(a), tostring(b)
        return a < b and -1 or (a > b and 1 or 0)
    end

    -- expressions -> closures over (row, agg) where agg is the group for aggregate functions
    local AGG = { COUNT = true, SUM = true, MAX = true, MIN = true, AVG = true, TOTAL = true, GROUP_CONCAT = true }
    local FN = {
        LENGTH = function(a) return a[1] ~= nil and #tostring(a[1]) or nil end,
        UPPER = function(a) return a[1] ~= nil and string.upper(tostring(a[1])) or nil end,
        LOWER = function(a) return a[1] ~= nil and string.lower(tostring(a[1])) or nil end,
        ABS = function(a) local n = num(a[1]); return n and math.abs(n) or nil end,
        ROUND = function(a) local n = num(a[1]); if not n then return nil end local d = num(a[2]) or 0; local m = 10 ^ d; return math.floor(n * m + 0.5) / m end,
        COALESCE = function(a) for _, v in ipairs(a) do if v ~= nil then return v end end return nil end,
        IFNULL = function(a) if a[1] ~= nil then return a[1] end return a[2] end,
        NULLIF = function(a) if cmp(a[1], a[2]) == 0 then return nil end return a[1] end,
        TYPEOF = function(a) if a[1] == nil then return "null" end if type(a[1]) == "number" then return math.floor(a[1]) == a[1] and "integer" or "real" end return "text" end,
        SUBSTR = function(a) local s = tostring(a[1] or ""); local st = num(a[2]) or 1; local len = num(a[3]); if len then return s:sub(st, st + len - 1) end return s:sub(st) end,
        REPLACE = function(a) return (tostring(a[1] or ""):gsub(tostring(a[2] or ""):gsub("%p", "%%%0"), (tostring(a[3] or ""):gsub("%%", "%%%%")))) end,
        TRIM = function(a) return (tostring(a[1] or ""):match("^%s*(.-)%s*$")) end,
        RANDOM = function() return math.random(-2 ^ 31, 2 ^ 31) end,
        MAX2 = function(a) local best; for _, v in ipairs(a) do if best == nil or cmp(v, best) == 1 then best = v end end return best end,
        MIN2 = function(a) local best; for _, v in ipairs(a) do if best == nil or cmp(v, best) == -1 then best = v end end return best end,
        LAST_INSERT_ROWID = function() return db and db.last_rowid or 0 end,
        CHANGES = function() return db and db.changes or 0 end,
        DATETIME = function() return os.date("!%Y-%m-%d %H:%M:%S") end,
        STRFTIME = function(a) local f = tostring(a[1] or ""); if f == "%s" then return tostring(os.time()) end return os.date("!" .. f) end,
    }
    local function aggregate(name, argfn, star, distinct, rows)
        if name == "COUNT" then
            if star then return #rows end
            local c, seen = 0, {}
            for _, r in ipairs(rows) do local v = argfn(r); if v ~= nil and not (distinct and seen[v]) then c = c + 1; seen[v] = true end end
            return c
        end
        local acc, n, parts = nil, 0, {}
        for _, r in ipairs(rows) do
            local v = argfn(r)
            if v ~= nil then
                n = n + 1
                if name == "SUM" or name == "TOTAL" or name == "AVG" then acc = (acc or 0) + (num(v) or 0)
                elseif name == "MAX" then if acc == nil or cmp(v, acc) == 1 then acc = v end
                elseif name == "MIN" then if acc == nil or cmp(v, acc) == -1 then acc = v end
                elseif name == "GROUP_CONCAT" then parts[#parts + 1] = tostring(v) end
            end
        end
        if name == "AVG" then return n > 0 and acc / n or nil end
        if name == "TOTAL" then return acc or 0 end
        if name == "GROUP_CONCAT" then return n > 0 and table.concat(parts, ",") or nil end
        return acc
    end

    local expr
    local function primary(p, ctx)
        local t = p:next()
        if not t then error("syntax error: incomplete input", 0) end
        if t.t == "num" then local v = t.v; return function() return v end end
        if t.t == "str" then local v = t.v; return function() return v end end
        if t.t == "op" and t.v == "(" then
            local e = expr(p, ctx); p:expect(")"); return e
        end
        if t.t == "op" and t.v == "-" then local e = primary(p, ctx); return function(r, g) local v = num(e(r, g)); return v and -v or nil end end
        if t.t == "op" and t.v == "+" then return primary(p, ctx) end
        if t.t == "op" and t.v == "*" then return function() return nil end, true end
        if t.t == "op" and t.v == "?" then return function() return nil end end
        if t.t == "word" or t.t == "id" then
            local up = string.upper(t.v)
            if t.t == "word" and up == "NULL" then return function() return nil end end
            if t.t == "word" and up == "TRUE" then return function() return 1 end end
            if t.t == "word" and up == "FALSE" then return function() return 0 end end
            if t.t == "word" and up == "NOT" then local e = primary(p, ctx); return function(r, g) return truthy(e(r, g)) and 0 or 1 end end
            if t.t == "word" and up == "CASE" then
                local base = (not p:is("WHEN")) and expr(p, ctx) or nil
                local whens = {}
                while p:accept("WHEN") do local c = expr(p, ctx); p:expect("THEN"); whens[#whens + 1] = { c, expr(p, ctx) } end
                local els = p:accept("ELSE") and expr(p, ctx) or nil
                p:expect("END")
                return function(r, g)
                    for _, w in ipairs(whens) do
                        local hit = base and cmp(base(r, g), w[1](r, g)) == 0 or (not base and truthy(w[1](r, g)))
                        if hit then return w[2](r, g) end
                    end
                    return els and els(r, g) or nil
                end
            end
            if p:is("(") and t.t == "word" then
                p:next()
                local star, distinct = false, false
                local args = {}
                if p:is("*") then p:next(); star = true
                else
                    if p:accept("DISTINCT") then distinct = true end
                    if not p:is(")") then repeat args[#args + 1] = expr(p, ctx) until not p:accept(",") end
                end
                p:expect(")")
                if AGG[up] then
                    ctx.has_agg = true
                    local a1 = args[1] or function() return nil end
                    return function(r, g) return aggregate(up, a1, star, distinct, g or { r }) end
                end
                local fn = FN[up] or (up == "MAX" and FN.MAX2) or (up == "MIN" and FN.MIN2)
                if not fn then error("no such function: " .. t.v, 0) end
                return function(r, g) local a = {}; for i, e in ipairs(args) do a[i] = e(r, g) end return fn(a) end
            end
            local col = t.v
            if p:is(".") then p:next(); col = p:ident() end
            local cl = string.lower(col)
            ctx.cols[#ctx.cols + 1] = col
            return function(r) if r == nil then return nil end return r[cl] end, false, col
        end
        error("near \"" .. tostring(t.v) .. "\": syntax error", 0)
    end
    local function mul(p, ctx)
        local e, star, name = primary(p, ctx)
        while p:is("*") or p:is("/") or p:is("%") do
            local op = p:next().v; local r = primary(p, ctx); local l = e
            e = function(row, g) local a, b = num(l(row, g)), num(r(row, g)); if not a or not b then return nil end
                if op == "*" then return a * b elseif op == "/" then return b ~= 0 and a / b or nil else return b ~= 0 and math.fmod(a, b) or nil end end
            star = false
        end
        return e, star, name
    end
    local function add(p, ctx)
        local e, star, name = mul(p, ctx)
        while p:is("+") or p:is("-") or p:is("||") do
            local op = p:next().v; local r = mul(p, ctx); local l = e
            if op == "||" then e = function(row, g) local a, b = l(row, g), r(row, g); if a == nil or b == nil then return nil end return tostring(a) .. tostring(b) end
            else e = function(row, g) local a, b = num(l(row, g)), num(r(row, g)); if not a or not b then return nil end return op == "+" and a + b or a - b end end
            star = false
        end
        return e, star, name
    end
    local function comparison(p, ctx)
        local e, star, name = add(p, ctx)
        local negate = false
        if p:is("NOT") and p:peek(1) and (string.upper(p:peek(1).v) == "LIKE" or string.upper(p:peek(1).v) == "IN" or string.upper(p:peek(1).v) == "BETWEEN" or string.upper(p:peek(1).v) == "GLOB") then p:next(); negate = true end
        local t = p:peek()
        if not t or (t.t ~= "op" and t.t ~= "word") then return e, star, name end
        local op = string.upper(t.v)
        local l = e
        if op == "=" or op == "==" or op == "!=" or op == "<>" or op == "<" or op == ">" or op == "<=" or op == ">=" then
            p:next(); local r = add(p, ctx)
            return function(row, g)
                local c = cmp(l(row, g), r(row, g))
                if c == nil then return nil end
                if op == "=" or op == "==" then return c == 0 and 1 or 0 elseif op == "!=" or op == "<>" then return c ~= 0 and 1 or 0
                elseif op == "<" then return c < 0 and 1 or 0 elseif op == ">" then return c > 0 and 1 or 0
                elseif op == "<=" then return c <= 0 and 1 or 0 else return c >= 0 and 1 or 0 end
            end
        elseif op == "LIKE" or op == "GLOB" then
            p:next(); local r = add(p, ctx)
            return function(row, g) local a, b = l(row, g), r(row, g); if a == nil or b == nil then return nil end local hit = like(a, b); if negate then hit = not hit end return hit and 1 or 0 end
        elseif op == "IS" then
            p:next(); local not_ = p:accept("NOT"); p:expect("NULL")
            return function(row, g) local isnull = l(row, g) == nil; if not_ then isnull = not isnull end return isnull and 1 or 0 end
        elseif op == "ISNULL" then p:next(); return function(row, g) return l(row, g) == nil and 1 or 0 end
        elseif op == "NOTNULL" then p:next(); return function(row, g) return l(row, g) ~= nil and 1 or 0 end
        elseif op == "IN" then
            p:next(); p:expect("("); local items = {}
            if not p:is(")") then repeat items[#items + 1] = expr(p, ctx) until not p:accept(",") end
            p:expect(")")
            return function(row, g) local v = l(row, g); local hit = false; for _, it in ipairs(items) do if cmp(v, it(row, g)) == 0 then hit = true; break end end if negate then hit = not hit end return hit and 1 or 0 end
        elseif op == "BETWEEN" then
            p:next(); local lo = add(p, ctx); p:expect("AND"); local hi = add(p, ctx)
            return function(row, g) local v = l(row, g); local a, b = cmp(v, lo(row, g)), cmp(v, hi(row, g)); if a == nil or b == nil then return nil end local hit = a >= 0 and b <= 0; if negate then hit = not hit end return hit and 1 or 0 end
        end
        return e, star, name
    end
    local function not_expr(p, ctx)
        if p:accept("NOT") then local e = not_expr(p, ctx); return function(r, g) local v = e(r, g); if v == nil then return nil end return truthy(v) and 0 or 1 end end
        return comparison(p, ctx)
    end
    local function and_expr(p, ctx)
        local e, star, name = not_expr(p, ctx)
        while p:accept("AND") do local r = not_expr(p, ctx); local l = e; e = function(row, g) return (truthy(l(row, g)) and truthy(r(row, g))) and 1 or 0 end; star = false end
        return e, star, name
    end
    expr = function(p, ctx)
        local e, star, name = and_expr(p, ctx)
        while p:accept("OR") do local r = and_expr(p, ctx); local l = e; e = function(row, g) return (truthy(l(row, g)) or truthy(r(row, g))) and 1 or 0 end; star = false end
        return e, star, name
    end

    local function table_of(name) local t = load_db().tables[string.lower(name)]; if not t then error("no such table: " .. name, 0) end return t end
    local function where_clause(p, ctx) if p:accept("WHERE") then return expr(p, ctx) end return nil end
    local function filter(t, w)
        local out = {}
        for _, r in ipairs(t.rows) do if not w or truthy(w(r)) then out[#out + 1] = r end end
        return out
    end
    local function tostr(v) if v == nil then return nil end if type(v) == "number" then if math.floor(v) == v and math.abs(v) < 2 ^ 53 then return string.format("%d", v) end return tostring(v) end if type(v) == "boolean" then return v and "1" or "0" end return tostring(v) end

    local function skip_type(p)
        -- a column type and its constraints up to the next `,` or `)` at depth 0
        local flags = {}
        local depth = 0
        while p:peek() do
            local t = p:peek()
            if depth == 0 and t.t == "op" and (t.v == "," or t.v == ")") then break end
            if t.t == "op" and t.v == "(" then depth = depth + 1 elseif t.t == "op" and t.v == ")" then depth = depth - 1 end
            p:next()
            local up = t.t == "word" and string.upper(t.v) or ""
            if up == "PRIMARY" then flags.pk = true
            elseif up == "UNIQUE" then flags.unique = true
            elseif up == "AUTOINCREMENT" then flags.auto = true
            elseif up == "NOT" and p:is("NULL") then p:next(); flags.notnull = true
            elseif up == "DEFAULT" then local ctx = { cols = {} }; local e = expr(p, ctx); flags.default = e
            elseif up == "INTEGER" or up == "INT" or up == "REAL" or up == "NUMERIC" or up == "FLOAT" or up == "DOUBLE" then flags.numeric = true end
        end
        return flags
    end

    local function create_table(p)
        local if_not = false
        if p:accept("IF") then p:expect("NOT"); p:expect("EXISTS"); if_not = true end
        local name = p:ident()
        local d = load_db()
        if d.tables[string.lower(name)] then if if_not then return end error("table " .. name .. " already exists", 0) end
        local t = { name = name, cols = {}, rows = {}, pk = nil, unique = {}, defaults = {}, numeric = {}, notnull = {} }
        p:expect("(")
        repeat
            if p:is("PRIMARY") then p:next(); p:expect("KEY"); p:expect("("); t.pk = string.lower(p:ident()); while p:accept(",") do p:ident() end p:expect(")")
            elseif p:is("UNIQUE") then p:next(); p:expect("("); t.unique[#t.unique + 1] = string.lower(p:ident()); while p:accept(",") do p:ident() end p:expect(")")
            elseif p:is("CONSTRAINT") or p:is("FOREIGN") or p:is("CHECK") then skip_type(p)
            else
                local col = p:ident()
                local f = skip_type(p)
                local cl = string.lower(col)
                t.cols[#t.cols + 1] = col
                if f.pk then t.pk = cl end
                if f.unique then t.unique[#t.unique + 1] = cl end
                if f.auto then t.auto = cl end
                if f.notnull then t.notnull[cl] = true end
                if f.numeric then t.numeric[cl] = true end
                if f.default then t.defaults[cl] = f.default(nil) end
            end
        until not p:accept(",")
        p:expect(")")
        if p:accept("WITHOUT") then p:ident() end
        d.tables[string.lower(name)] = t
        save()
    end
    local function check_unique(t, row, skip)
        local keys = {}
        if t.pk then keys[#keys + 1] = t.pk end
        for _, u in ipairs(t.unique) do keys[#keys + 1] = u end
        for _, k in ipairs(keys) do
            if row[k] ~= nil then
                for _, other in ipairs(t.rows) do
                    if other ~= skip and cmp(other[k], row[k]) == 0 then return other, k end
                end
            end
        end
        return nil
    end
    local function insert(p, mode)
        if mode == "INSERT" and p:accept("OR") then mode = string.upper(p:next().v) == "REPLACE" and "REPLACE" or "IGNORE" end
        p:expect("INTO")
        local t = table_of(p:ident())
        local cols = {}
        if p:accept("(") then repeat cols[#cols + 1] = string.lower(p:ident()) until not p:accept(",") p:expect(")")
        else for i, c in ipairs(t.cols) do cols[i] = string.lower(c) end end
        local d = load_db()
        local count = 0
        if p:accept("DEFAULT") then p:expect("VALUES"); local row = {}; for k, v in pairs(t.defaults) do row[k] = v end t.rows[#t.rows + 1] = row; count = 1
        else
            p:expect("VALUES")
            repeat
                p:expect("(")
                local row = {}
                for k, v in pairs(t.defaults) do row[k] = v end
                local i = 1
                if not p:is(")") then
                    repeat
                        local ctx = { cols = {} }
                        local e = expr(p, ctx)
                        local col = cols[i]
                        if not col then error("table " .. t.name .. " has " .. #cols .. " columns but more values were supplied", 0) end
                        row[col] = e(nil)
                        i = i + 1
                    until not p:accept(",")
                end
                p:expect(")")
                if t.auto and row[t.auto] == nil then
                    local best = 0
                    for _, r in ipairs(t.rows) do best = math.max(best, num(r[t.auto]) or 0) end
                    row[t.auto] = best + 1
                elseif t.pk and row[t.pk] == nil and t.numeric[t.pk] then
                    local best = 0
                    for _, r in ipairs(t.rows) do best = math.max(best, num(r[t.pk]) or 0) end
                    row[t.pk] = best + 1
                end
                for k in pairs(t.notnull) do if row[k] == nil then error("NOT NULL constraint failed: " .. t.name .. "." .. k, 0) end end
                local clash, key = check_unique(t, row)
                if clash then
                    if mode == "REPLACE" then
                        for i2, r in ipairs(t.rows) do if r == clash then table.remove(t.rows, i2); break end end
                    elseif mode == "IGNORE" then row = nil
                    else error("UNIQUE constraint failed: " .. t.name .. "." .. key, 0) end
                end
                if row then t.rows[#t.rows + 1] = row; count = count + 1; d.last_rowid = #t.rows end
            until not p:accept(",")
        end
        d.changes = count
        save()
    end
    local function select(p)
        local distinct = p:accept("DISTINCT")
        p:accept("ALL")
        local ctx = { cols = {} }
        local fields = {}
        repeat
            if p:is("*") then p:next(); fields[#fields + 1] = { star = true }
            else
                local e, _, colname = expr(p, ctx)
                local alias = colname
                if p:accept("AS") then alias = p:ident() elseif p:peek() and (p:peek().t == "id" or (p:peek().t == "word" and not ({ FROM = 1, WHERE = 1, ORDER = 1, LIMIT = 1, GROUP = 1 })[string.upper(p:peek().v)])) then alias = p:ident() end
                fields[#fields + 1] = { e = e, name = alias or ("column" .. #fields) }
            end
        until not p:accept(",")
        local rows, t = {}, nil
        if p:accept("FROM") then
            t = table_of(p:ident())
            if p:peek() and (p:peek().t == "id" or (p:peek().t == "word" and not ({ WHERE = 1, ORDER = 1, LIMIT = 1, GROUP = 1 })[string.upper(p:peek().v)])) then p:ident() end
            rows = filter(t, where_clause(p, ctx))
        else rows = { {} } end
        local groups = nil
        if p:accept("GROUP") then
            p:expect("BY")
            local keys = {}
            repeat keys[#keys + 1] = expr(p, ctx) until not p:accept(",")
            groups = {}
            local order = {}
            for _, r in ipairs(rows) do
                local parts = {}
                for i, k in ipairs(keys) do parts[i] = tostring(k(r)) end
                local id = table.concat(parts, "\1")
                if not groups[id] then groups[id] = {}; order[#order + 1] = id end
                groups[id][#groups[id] + 1] = r
            end
            local g = {}
            for _, id in ipairs(order) do g[#g + 1] = groups[id] end
            groups = g
            if p:accept("HAVING") then local h = expr(p, ctx); local kept = {}; for _, gr in ipairs(groups) do if truthy(h(gr[1], gr)) then kept[#kept + 1] = gr end end groups = kept end
        elseif ctx.has_agg then groups = { rows }
        end
        local out = {}
        local function emit(r, g)
            local o = {}
            for _, f in ipairs(fields) do
                if f.star then
                    if t then for _, c in ipairs(t.cols) do o[c] = r[string.lower(c)] end end
                else o[f.name] = f.e(r, g) end
            end
            out[#out + 1] = { o, r }
        end
        if groups then for _, g in ipairs(groups) do emit(g[1] or {}, g) end else for _, r in ipairs(rows) do emit(r) end end
        if p:accept("ORDER") then
            p:expect("BY")
            local keys = {}
            repeat
                local e, _, colname = expr(p, ctx)
                local desc = false
                if p:accept("DESC") then desc = true else p:accept("ASC") end
                keys[#keys + 1] = { e = e, desc = desc, name = colname }
            until not p:accept(",")
            table.sort(out, function(a, b)
                for _, k in ipairs(keys) do
                    local va = a[1][k.name] ~= nil and a[1][k.name] or k.e(a[2]); local vb = b[1][k.name] ~= nil and b[1][k.name] or k.e(b[2])
                    local c = cmp(va, vb)
                    if c == nil then c = (va == nil and vb ~= nil) and -1 or ((va ~= nil and vb == nil) and 1 or 0) end
                    if c ~= 0 then if k.desc then return c > 0 end return c < 0 end
                end
                return false
            end)
        end
        if p:accept("LIMIT") then
            local lim = expr(p, ctx)(nil)
            local off = 0
            if p:accept("OFFSET") then off = expr(p, ctx)(nil) elseif p:accept(",") then off = lim; lim = expr(p, ctx)(nil) end
            local sliced = {}
            for i = (num(off) or 0) + 1, math.min(#out, (num(off) or 0) + (num(lim) or 0)) do sliced[#sliced + 1] = out[i] end
            if (num(lim) or 0) < 0 then sliced = {}; for i = (num(off) or 0) + 1, #out do sliced[#sliced + 1] = out[i] end end
            out = sliced
        end
        local result = {}
        local seen = {}
        for _, pair in ipairs(out) do
            local row = pair[1]
            if distinct then
                local parts = {}
                for k, v in pairs(row) do parts[#parts + 1] = k .. "=" .. tostring(v) end
                table.sort(parts)
                local id = table.concat(parts, "\1")
                if seen[id] then row = nil else seen[id] = true end
            end
            if row then result[#result + 1] = row end
        end
        return result
    end
    local function update(p)
        p:accept("OR"); if p:is("REPLACE") or p:is("IGNORE") then p:next() end
        local t = table_of(p:ident())
        p:expect("SET")
        local sets = {}
        repeat local col = string.lower(p:ident()); p:expect("="); local ctx = { cols = {} }; sets[#sets + 1] = { col, expr(p, ctx) } until not p:accept(",")
        local ctx = { cols = {} }
        local rows = filter(t, where_clause(p, ctx))
        for _, r in ipairs(rows) do
            local new = {}
            for k, v in pairs(r) do new[k] = v end
            for _, s in ipairs(sets) do new[s[1]] = s[2](r) end
            local clash, key = check_unique(t, new, r)
            if clash then error("UNIQUE constraint failed: " .. t.name .. "." .. key, 0) end
            for k in pairs(r) do r[k] = nil end
            for k, v in pairs(new) do r[k] = v end
        end
        load_db().changes = #rows
        save()
    end
    local function delete(p)
        p:expect("FROM")
        local t = table_of(p:ident())
        local ctx = { cols = {} }
        local w = where_clause(p, ctx)
        local kept, removed = {}, 0
        for _, r in ipairs(t.rows) do if w and not truthy(w(r)) then kept[#kept + 1] = r else removed = removed + 1 end end
        t.rows = kept
        load_db().changes = removed
        save()
    end

    local function run(q)
        local p = parser(lex(q))
        local results = nil
        while p:peek() do
            if p:accept(";") then
            else
                local kw = string.upper(p:next().v)
                if kw == "CREATE" then
                    local unique = p:accept("UNIQUE")
                    if p:accept("TABLE") then create_table(p)
                    elseif p:accept("INDEX") then
                        if p:accept("IF") then p:expect("NOT"); p:expect("EXISTS") end
                        local name = p:ident(); p:expect("ON"); local tn = p:ident(); table_of(tn)
                        p:expect("("); local cols = {}; repeat cols[#cols + 1] = string.lower(p:ident()); p:accept("ASC"); p:accept("DESC") until not p:accept(",") p:expect(")")
                        load_db().indexes[string.lower(name)] = { table = string.lower(tn), cols = cols, unique = unique }
                        if unique then local t = table_of(tn); t.unique[#t.unique + 1] = cols[1] end
                        save()
                    else error("syntax error after CREATE", 0) end
                elseif kw == "DROP" then
                    local what = string.upper(p:next().v)
                    local if_exists = false
                    if p:accept("IF") then p:expect("EXISTS"); if_exists = true end
                    local name = string.lower(p:ident())
                    local d = load_db()
                    if what == "TABLE" then
                        if not d.tables[name] and not if_exists then error("no such table: " .. name, 0) end
                        d.tables[name] = nil
                        for k, ix in pairs(d.indexes) do if ix.table == name then d.indexes[k] = nil end end
                    elseif what == "INDEX" then
                        if not d.indexes[name] and not if_exists then error("no such index: " .. name, 0) end
                        d.indexes[name] = nil
                    end
                    save()
                elseif kw == "INSERT" or kw == "REPLACE" then insert(p, kw)
                elseif kw == "SELECT" then results = select(p)
                elseif kw == "UPDATE" then update(p)
                elseif kw == "DELETE" then delete(p)
                elseif kw == "BEGIN" or kw == "COMMIT" or kw == "END" or kw == "ROLLBACK" or kw == "VACUUM" then p:accept("TRANSACTION")
                elseif kw == "PRAGMA" then while p:peek() and not p:is(";") do p:next() end
                elseif kw == "ALTER" then
                    p:expect("TABLE"); local t = table_of(p:ident())
                    if p:accept("ADD") then p:accept("COLUMN"); local col = p:ident(); local f = skip_type(p); t.cols[#t.cols + 1] = col; if f.default then local v = f.default(nil); t.defaults[string.lower(col)] = v; for _, r in ipairs(t.rows) do r[string.lower(col)] = v end end
                    elseif p:accept("RENAME") then p:expect("TO"); local nn = p:ident(); local d = load_db(); d.tables[string.lower(t.name)] = nil; t.name = nn; d.tables[string.lower(nn)] = t end
                    save()
                else error("near \"" .. kw .. "\": syntax error", 0) end
            end
        end
        return results
    end

    local function query(q, typed)
        last_error = nil
        local ok, res = pcall(run, tostring(q))
        if not ok then last_error = tostring(res); return false end
        if res == nil then return nil end
        if #res == 0 then return nil end
        if not typed then for _, row in ipairs(res) do for k, v in pairs(row) do row[k] = tostr(v) end end end
        for _, row in ipairs(res) do for k, v in pairs(row) do if v == nil then row[k] = "NULL" end end end
        return res
    end
    function sql.Query(q) return query(q, false) end
    function sql.QueryTyped(q) return query(q, true) end
    function sql.QueryRow(q, row) local r = query(q, false); if r == false then return false end return r and r[tonumber(row) or 1] or nil end
    function sql.QueryValue(q)
        local r = query(q, false)
        if r == false then return false end
        if not r or not r[1] then return nil end
        local _, v = next(r[1])
        return v
    end
    function sql.LastError() return last_error end
    function sql.TableExists(name) return load_db().tables[string.lower(tostring(name))] ~= nil end
    function sql.IndexExists(name) return load_db().indexes[string.lower(tostring(name))] ~= nil end
    function sql.SQLStr(s, noQuotes) local e = tostring(s):gsub("'", "''"); if noQuotes then return e end return "'" .. e .. "'" end
    function sql.Begin() in_tx = true end
    function sql.Commit() in_tx = false; if dirty then save() end end
end

------------------------------------------------------------------------ Tool ghost entities (stool preview: a non-solid translucent prop at the owner's eye trace)
do
    local TL = __glua_classes.Tool
    if TL then
        function TL:ReleaseGhostEntity()
            if IsValid(self.GhostEntity) then self.GhostEntity:Remove() end
            self.GhostEntity = nil
        end
        function TL:MakeGhostEntity(model, pos, ang)
            self:ReleaseGhostEntity()
            if not model or model == "" then return end
            local g
            if CLIENT and ClientsideModel then g = ClientsideModel(model, RENDERGROUP_TRANSLUCENT) else g = ents.Create("prop_physics") end
            if not IsValid(g) then return end
            if g.SetModel then g:SetModel(model) end
            g:SetPos(pos or Vector(0, 0, 0)); g:SetAngles(ang or Angle(0, 0, 0))
            if g.Spawn and SERVER then g:Spawn() end
            if g.SetNotSolid then g:SetNotSolid(true) end
            if g.SetMoveType then g:SetMoveType(MOVETYPE_NONE) end
            if g.SetCollisionGroup then g:SetCollisionGroup(COLLISION_GROUP_WORLD) end
            g:SetRenderMode(RENDERMODE_TRANSCOLOR)
            g:SetColor(Color(255, 255, 255, 150))
            local p = g.GetPhysicsObject and g:GetPhysicsObject()
            if IsValid(p) then p:EnableMotion(false); p:EnableCollisions(false) end
            self.GhostEntity = g
        end
        function TL:StartGhostEntity(ent)
            if not IsValid(ent) then return end
            self:MakeGhostEntity(ent:GetModel(), ent:GetPos(), ent:GetAngles())
        end
        function TL:UpdateGhostEntity()
            local g = self.GhostEntity
            if not IsValid(g) then return end
            local owner = self:GetOwner()
            if not IsValid(owner) then return end
            local tr = owner:GetEyeTrace()
            if not tr or not tr.Hit then return end
            local ang = tr.HitNormal:Angle(); ang.p = ang.p + 90
            g:SetPos(tr.HitPos); g:SetAngles(ang)
        end
        function TL:UpdateData()
            if self.CheckObjects then self:CheckObjects() end
            self:UpdateGhostEntity()
        end
    end
end

------------------------------------------------------------------------ drive (the game's includes/modules/drive.lua): driving methods over entities
drive = drive or {}
do
    local Type = {}
    local ids, names = {}, {}
    local function id_of(name)
        if util and util.NetworkStringToID and util.AddNetworkString then
            if SERVER then util.AddNetworkString(name) end
            local id = util.NetworkStringToID(name)
            if id and id ~= 0 then return id end
        end
        if not ids[name] then ids[name] = #names + 1; names[ids[name]] = name end
        return ids[name]
    end
    local function name_of(id) return names[id] or (util and util.NetworkIDToString and util.NetworkIDToString(id)) end
    function drive.Register(name, tbl, base)
        Type[name] = tbl
        if base then
            Type[base] = Type[base] or (baseclass and baseclass.Get and baseclass.Get(base))
            if Type[base] then setmetatable(Type[name], { __index = Type[base] }) end
        end
        id_of(name)
        if baseclass and baseclass.Set then baseclass.Set(name, Type[name]) end
    end
    function drive.PlayerStartDriving(ply, ent, mode)
        if not Type[mode] then ErrorNoHalt("Unknown drive type " .. tostring(mode) .. "!\n") return end
        ply:SetDrivingEntity(ent, id_of(mode))
    end
    function drive.PlayerStopDriving(ply) ply:SetDrivingEntity(nil) end
    function drive.GetMethod(ply)
        if not IsValid(ply) or not ply:IsDrivingEntity() then return end
        local ent, modeid = ply:GetDrivingEntity(), ply:GetDrivingMode()
        if not IsValid(ent) or (modeid or 0) == 0 then return end
        local method = ply.m_CurrentDriverMethod
        if method and method.Entity == ent and method.ModeID == modeid then return method end
        local tbl = Type[name_of(modeid) or ""]
        if not tbl then return end
        method = setmetatable({ Entity = ent, Player = ply, ModeID = modeid }, { __index = tbl })
        ply.m_CurrentDriverMethod = method
        if method.Init then method:Init() end
        return method
    end
    function drive.DestroyMethod(ply) if IsValid(ply) then ply.m_CurrentDriverMethod = nil end end
    function drive.Start(ply, ent)
        if SERVER then
            ply:SetViewEntity(ent)
            ply.m_PreDriveEyeAngles = ply:EyeAngles()
            ply.m_PreDriveObserveMode = ply:GetObserverMode()
            ply:SetEyeAngles(ent:GetAngles())
            ply:DrawWorldModel(false)
        end
    end
    function drive.CreateMove(cmd) local m = drive.GetMethod(LocalPlayer()); if not m then return end if m.SetupControls then m:SetupControls(cmd) end return true end
    function drive.CalcView(ply, view) local m = drive.GetMethod(ply); if not m then return end if m.CalcView then m:CalcView(view) end return true end -- @hook DRIVE:CalcView
    function drive.StartMove(ply, mv, cmd) local m = drive.GetMethod(ply); if not m then return end if m.StartMove then m:StartMove(mv, cmd) end return true end
    function drive.Move(ply, mv) local m = drive.GetMethod(ply); if not m then return end if m.Move then m:Move(mv) end return true end
    function drive.FinishMove(ply, mv)
        local m = drive.GetMethod(ply); if not m then return end
        if m.FinishMove then m:FinishMove(mv) end
        if m.StopDriving then drive.PlayerStopDriving(ply) end
        return true
    end
    function drive.End(ply, ent)
        if SERVER and IsValid(ply) then
            if ply.m_PreDriveEyeAngles ~= nil then ply:SetEyeAngles(ply.m_PreDriveEyeAngles); ply.m_PreDriveEyeAngles = nil end
            if ply.m_PreDriveObserveMode ~= nil then ply:SetObserverMode(ply.m_PreDriveObserveMode); ply.m_PreDriveObserveMode = nil end
            ply:SetViewEntity(nil); ply:DrawWorldModel(true)
        end
        drive.DestroyMethod(ply)
    end
    local PL = __glua_classes.Player
    if PL then
        if not PL.IsDrivingEntity then function PL:IsDrivingEntity() return IsValid(self:GetDrivingEntity()) end end
        if not PL.GetDrivingMode then function PL:GetDrivingMode() local v = rawget(self, "__vars"); return v and v.__driving_mode or 0 end end
    end
end

------------------------------------------------------------------------ debugoverlay: shapes kept with their lifetime (read back with debugoverlay.__shapes; no world drawing from Lua here)
debugoverlay = debugoverlay or {}
do
    local shapes = {}
    local function add(kind, lifetime, t) t.kind = kind; t.until_ = CurTime() + (lifetime or 1); shapes[#shapes + 1] = t return t end
    function debugoverlay.Axis(origin, ang, size, lifetime, ignoreZ) add("axis", lifetime, { origin = origin, angles = ang, size = size, ignoreZ = ignoreZ }) end
    function debugoverlay.Box(origin, mins, maxs, lifetime, color, ignoreZ) add("box", lifetime, { origin = origin, mins = mins, maxs = maxs, color = color, ignoreZ = ignoreZ }) end
    function debugoverlay.BoxAngles(origin, mins, maxs, ang, lifetime, color, ignoreZ) add("box_angles", lifetime, { origin = origin, mins = mins, maxs = maxs, angles = ang, color = color, ignoreZ = ignoreZ }) end
    function debugoverlay.Cross(pos, size, lifetime, color, ignoreZ) add("cross", lifetime, { origin = pos, size = size, color = color, ignoreZ = ignoreZ }) end
    function debugoverlay.EntityTextAtPosition(pos, line, text, lifetime, color) add("entity_text", lifetime, { origin = pos, line = line, text = text, color = color }) end
    function debugoverlay.Grid(pos) add("grid", 1, { origin = pos }) end
    function debugoverlay.Line(a, b, lifetime, color, ignoreZ) add("line", lifetime, { a = a, b = b, color = color, ignoreZ = ignoreZ }) end
    function debugoverlay.ScreenText(x, y, text, lifetime, color) add("screen_text", lifetime, { x = x, y = y, text = text, color = color }) end
    function debugoverlay.Sphere(origin, size, lifetime, color, ignoreZ) add("sphere", lifetime, { origin = origin, size = size, color = color, ignoreZ = ignoreZ }) end
    function debugoverlay.SweptBox(start, endp, mins, maxs, ang, lifetime, color) add("swept_box", lifetime, { a = start, b = endp, mins = mins, maxs = maxs, angles = ang, color = color }) end
    function debugoverlay.Text(pos, text, lifetime, viewCheck) add("text", lifetime, { origin = pos, text = text, viewCheck = viewCheck }) end
    function debugoverlay.Triangle(a, b, c, lifetime, color, ignoreZ) add("triangle", lifetime, { a = a, b = b, c = c, color = color, ignoreZ = ignoreZ }) end
    function debugoverlay.__shapes()
        local now = CurTime()
        for i = #shapes, 1, -1 do if shapes[i].until_ < now then table.remove(shapes, i) end end
        return shapes
    end
end

------------------------------------------------------------------------ navmesh editing, CNavArea attributes / places / spots / connections, CNavLadder, PathFollower / locomotion / NextBot extras
do
    local NAV = __glua_classes.CNavArea
    local attrs, places, hiding, damaging, extra_links, cut_links, corner_moves, removed = {}, {}, {}, {}, {}, {}, {}, {}
    local function id(a) return a.__id end
    if NAV then
        function NAV:GetAttributes() return attrs[id(self)] or 0 end
        function NAV:SetAttributes(f) attrs[id(self)] = f end
        function NAV:AddAttributes(f) attrs[id(self)] = bit.bor(attrs[id(self)] or 0, f) end
        function NAV:RemoveAttributes(f) attrs[id(self)] = bit.band(attrs[id(self)] or 0, bit.bnot(f)) end
        function NAV:HasAttributes(f) return bit.band(attrs[id(self)] or 0, f) ~= 0 end
        function NAV:GetPlace() return places[id(self)] end
        function NAV:SetPlace(p) places[id(self)] = p return true end
        function NAV:AddHidingSpot(pos, flags) local h = hiding[id(self)] or {}; hiding[id(self)] = h; h[#h + 1] = { pos = pos, flags = flags or 0 } end
        function NAV:GetHidingSpots(flag)
            local out = {}
            for _, h in ipairs(hiding[id(self)] or {}) do if flag == nil or flag == 0 or bit.band(h.flags, flag) ~= 0 then out[#out + 1] = h.pos end end
            return out
        end
        function NAV:GetExposedSpots() return self:GetHidingSpots(8) end
        function NAV:GetSpotEncounters() return {} end
        function NAV:IsDamaging() return (damaging[id(self)] or 0) > CurTime() end
        function NAV:MarkAsDamaging(duration) damaging[id(self)] = CurTime() + (duration or 1) end
        function NAV:GetLightIntensity() return 1 end
        function NAV:IsCoplanar(other) return math.abs(self:GetCenter().z - other:GetCenter().z) < 1 end
        function NAV:PlaceOnGround(corner) return true end
        function NAV:SetCorner(corner, pos) corner_moves[id(self)] = corner_moves[id(self)] or {}; corner_moves[id(self)][corner] = pos end
        local base_corner = NAV.GetCorner
        function NAV:GetCorner(c) local m = corner_moves[id(self)]; if m and m[c] then return m[c] end return base_corner(self, c) end
        function NAV:Remove() removed[id(self)] = true end
        local base_valid = NAV.IsValid
        function NAV:IsValid() if removed[id(self)] then return false end return base_valid and base_valid(self) or true end
        function NAV:ConnectTo(other) local l = extra_links[id(self)] or {}; extra_links[id(self)] = l; l[id(other)] = other; if cut_links[id(self)] then cut_links[id(self)][id(other)] = nil end end
        function NAV:Disconnect(other) local c = cut_links[id(self)] or {}; cut_links[id(self)] = c; c[id(other)] = true; if extra_links[id(self)] then extra_links[id(self)][id(other)] = nil end end
        local base_adj = NAV.GetAdjacentAreas
        function NAV:GetAdjacentAreas()
            local out = {}
            local cut = cut_links[id(self)] or {}
            for _, a in ipairs(base_adj(self)) do if not cut[id(a)] and not removed[id(a)] then out[#out + 1] = a end end
            for _, a in pairs(extra_links[id(self)] or {}) do out[#out + 1] = a end
            return out
        end
        local base_conn = NAV.IsConnected
        function NAV:IsConnected(other)
            if cut_links[id(self)] and cut_links[id(self)][id(other)] then return false end
            if extra_links[id(self)] and extra_links[id(self)][id(other)] then return true end
            return base_conn(self, other)
        end
        function NAV:GetIncomingConnectionDistances(dir) local out = {}; for _, a in ipairs(self:GetIncomingConnections(dir)) do out[#out + 1] = a:GetCenter():Distance(self:GetCenter()) end return out end
        function NAV:GetLadders() local out = {}; for _, l in pairs(navmesh.__ladders or {}) do if l.__top_area == self or l.__bottom_area == self then out[#out + 1] = l end end return out end
        function NAV:GetLaddersAtSide(side) local out = {}; for _, l in ipairs(self:GetLadders()) do if (side == 0) == (l.__top_area == self) then out[#out + 1] = l end end return out end
        function NAV:Draw() debugoverlay.Box(self:GetCenter(), Vector(-self:GetSizeX() / 2, -self:GetSizeY() / 2, 0), Vector(self:GetSizeX() / 2, self:GetSizeY() / 2, 2), 0.1, Color(0, 255, 0, 60), true) end
        function NAV:DrawSpots() for _, p in ipairs(self:GetHidingSpots()) do debugoverlay.Cross(p, 8, 0.1, Color(255, 255, 0), true) end end
    end
    -- CNavLadder: Lua-created ladders (the mesh has none of its own)
    local LADDER = {}
    LADDER.__index = LADDER
    LADDER.__tostring = function(l) return "CNavLadder [" .. l.__id .. "]" end
    __glua_classes.CNavLadder = LADDER
    navmesh = navmesh or {}
    navmesh.__ladders = {}
    local next_ladder = 1
    function LADDER:IsValid() return navmesh.__ladders[self.__id] == self end
    function LADDER:Remove() navmesh.__ladders[self.__id] = nil end
    function LADDER:GetID() return self.__id end
    function LADDER:GetTop() return self.__top end
    function LADDER:GetBottom() return self.__bottom end
    function LADDER:GetLength() return self.__top:Distance(self.__bottom) end
    function LADDER:GetWidth() return self.__width end
    function LADDER:GetNormal() return self.__normal end
    function LADDER:GetPosAtHeight(h) local f = math.Clamp((h - self.__bottom.z) / math.max(self.__top.z - self.__bottom.z, 1e-3), 0, 1); return self.__bottom + (self.__top - self.__bottom) * f end
    for _, side in ipairs({ "TopForward", "TopLeft", "TopRight", "TopBehind", "Bottom" }) do
        LADDER["Get" .. side .. "Area"] = function(self) return self["__" .. string.lower(side) .. "_area"] end
        LADDER["Set" .. side .. "Area"] = function(self, a) self["__" .. string.lower(side) .. "_area"] = a; if side ~= "Bottom" then self.__top_area = a end end
    end
    function LADDER:ConnectTo(area, side) if side == 0 then self:SetTopForwardArea(area) elseif side == 1 then self:SetTopLeftArea(area) elseif side == 2 then self:SetTopRightArea(area) elseif side == 3 then self:SetTopBehindArea(area) else self:SetBottomArea(area) end end
    function LADDER:Disconnect(area) for _, k in ipairs({ "__topforward_area", "__topleft_area", "__topright_area", "__topbehind_area", "__bottom_area" }) do if self[k] == area then self[k] = nil end end if self.__top_area == area then self.__top_area = nil end end
    function LADDER:IsConnectedAtSide(area, side) if side == 4 then return self.__bottom_area == area end return self.__top_area == area end
    function navmesh.CreateNavLadder(top, bottom, forward, width)
        local l = setmetatable({ __id = next_ladder, __top = top, __bottom = bottom, __normal = forward, __width = width or 32 }, LADDER)
        navmesh.__ladders[next_ladder] = l; next_ladder = next_ladder + 1
        return l
    end
    function navmesh.GetNavLadderByID(i) return navmesh.__ladders[i] or NULL end
    -- navmesh editing state: seeds, marks, generation and the file are kept, generation is the server's own mesh build
    local seeds, marked_area, marked_ladder, spawn_name, generating = {}, nil, nil, "info_player_start", false
    function navmesh.AddWalkableSeed(pos, normal) seeds[#seeds + 1] = { pos = pos, normal = normal } end
    function navmesh.ClearWalkableSeeds() seeds = {} end
    function navmesh.BeginGeneration() generating = true; timer.Simple(0, function() generating = false end) return true end
    function navmesh.IsGenerating() return generating end
    function navmesh.Load() return navmesh.IsLoaded() end
    function navmesh.Save() return navmesh.IsLoaded() end
    function navmesh.Reset() attrs, places, hiding, extra_links, cut_links, corner_moves, removed = {}, {}, {}, {}, {}, {}, {}; navmesh.__ladders = {} end
    function navmesh.GetMarkedArea() return marked_area end
    function navmesh.SetMarkedArea(a) marked_area = a end
    function navmesh.GetMarkedLadder() return marked_ladder end
    function navmesh.SetMarkedLadder(l) marked_ladder = l end
    function navmesh.GetPlayerSpawnName() return spawn_name end
    function navmesh.SetPlayerSpawnName(n) spawn_name = n end
    function navmesh.GetEditCursorPosition()
        local ply = player and player.GetAll and player.GetAll()[1]
        if IsValid(ply) then local tr = ply:GetEyeTrace(); if tr and tr.Hit then return tr.HitPos end end
        return Vector(0, 0, 0)
    end
    -- a Lua-made area: a flat rectangle between two corners with the game's area interface
    local LUANAV = {}
    LUANAV.__index = LUANAV
    LUANAV.__tostring = function(a) return "CNavArea [" .. a.__id .. "]" end
    local next_lua_area = -1
    local lua_areas = {}
    function navmesh.CreateNavArea(c1, c2)
        local mins = Vector(math.min(c1.x, c2.x), math.min(c1.y, c2.y), math.min(c1.z, c2.z))
        local maxs = Vector(math.max(c1.x, c2.x), math.max(c1.y, c2.y), math.max(c1.z, c2.z))
        local a = setmetatable({ __id = next_lua_area, __mins = mins, __maxs = maxs, __links = {} }, LUANAV)
        lua_areas[next_lua_area] = a; next_lua_area = next_lua_area - 1
        return a
    end
    function LUANAV:IsValid() return lua_areas[self.__id] == self end
    function LUANAV:Remove() lua_areas[self.__id] = nil end
    function LUANAV:GetID() return self.__id end
    function LUANAV:GetCenter() return (self.__mins + self.__maxs) / 2 end
    function LUANAV:GetSizeX() return self.__maxs.x - self.__mins.x end
    function LUANAV:GetSizeY() return self.__maxs.y - self.__mins.y end
    function LUANAV:GetZ(pos) return self.__mins.z end
    function LUANAV:GetCorner(c) local m, M = self.__mins, self.__maxs; if c == 0 then return Vector(m.x, m.y, m.z) elseif c == 1 then return Vector(M.x, m.y, m.z) elseif c == 2 then return Vector(M.x, M.y, m.z) end return Vector(m.x, M.y, m.z) end
    function LUANAV:SetCorner(c, pos) if c == 0 then self.__mins = Vector(pos.x, pos.y, pos.z) elseif c == 2 then self.__maxs = Vector(pos.x, pos.y, pos.z) end end
    function LUANAV:Contains(pos) return pos.x >= self.__mins.x and pos.x <= self.__maxs.x and pos.y >= self.__mins.y and pos.y <= self.__maxs.y end
    function LUANAV:GetClosestPointOnArea(pos) return Vector(math.Clamp(pos.x, self.__mins.x, self.__maxs.x), math.Clamp(pos.y, self.__mins.y, self.__maxs.y), self.__mins.z) end
    function LUANAV:GetRandomPoint() return Vector(math.Rand(self.__mins.x, self.__maxs.x), math.Rand(self.__mins.y, self.__maxs.y), self.__mins.z) end
    function LUANAV:ConnectTo(other) self.__links[other] = true end
    function LUANAV:Disconnect(other) self.__links[other] = nil end
    function LUANAV:IsConnected(other) return self.__links[other] == true end
    function LUANAV:GetAdjacentAreas() local out = {}; for a in pairs(self.__links) do out[#out + 1] = a end return out end
    function LUANAV:GetAdjacentCount() return #self:GetAdjacentAreas() end
    function LUANAV:IsFlat() return true end
    function LUANAV:IsBlocked() return false end
    function LUANAV:IsUnderwater() return false end
    function LUANAV:GetAttributes() return self.__attrs or 0 end
    function LUANAV:SetAttributes(f) self.__attrs = f end
    function LUANAV:HasAttributes(f) return bit.band(self.__attrs or 0, f) ~= 0 end
    function LUANAV:PlaceOnGround() return true end
    function LUANAV:Draw() debugoverlay.Box(self:GetCenter(), self.__mins - self:GetCenter(), self.__maxs - self:GetCenter() + Vector(0, 0, 2), 0.1, Color(0, 255, 0, 60), true) end
    -- PathFollower drawing / hindrance, locomotion and NextBot extras
    local PF = __glua_classes.PathFollower
    if PF then
        function PF:Draw() local segs = self.segs or {}; for i = 2, #segs do debugoverlay.Line(Vector(segs[i - 1].pos.x, segs[i - 1].pos.y, segs[i - 1].pos.z), Vector(segs[i].pos.x, segs[i].pos.y, segs[i].pos.z), 0.1, Color(255, 255, 0), true) end end
        function PF:GetHindrance() local g = self.GetCurrentGoal and self:GetCurrentGoal(); if not g or not g.pos then return NULL end for _, e in ipairs(ents.FindInSphere(Vector(g.pos.x, g.pos.y, g.pos.z), 24)) do if IsValid(e) and not e:IsPlayer() and not e:IsWorld() and e:GetMoveType() == MOVETYPE_VPHYSICS then return e end end return NULL end
    end
    local LOCO = __glua_classes.CLuaLocomotion
    if LOCO then
        function LOCO:GetCurrentAcceleration() local v = self:GetVelocity(); local last = rawget(self, "__last_vel") or v; rawset(self, "__last_vel", v); return (v - last) / math.max(FrameTime(), 1e-3) end
        function LOCO:GetNextBot() return self.__ent and self.__ent.Entity or NULL end
        function LOCO:IsUsingLadder() return rawget(self, "__on_ladder") == true end
    end
    local NB = __glua_classes.NextBot
    if NB then
        function NB:GetSolidMask() local v = rawget(self, "__vars"); return v and v.__solid_mask or MASK_NPCSOLID end
        function NB:SetSolidMask(m) local v = rawget(self, "__vars"); if v then v.__solid_mask = m end end
        function NB:BecomeRagdoll(dmg)
            local r = ents.Create("prop_ragdoll")
            if not IsValid(r) then return NULL end
            r:SetModel(self:GetModel()); r:SetPos(self:GetPos()); r:SetAngles(self:GetAngles()); r:SetSkin(self:GetSkin()); r:Spawn()
            if dmg and dmg.GetDamageForce then local p = r:GetPhysicsObject(); if IsValid(p) then p:SetVelocity(dmg:GetDamageForce() / 100) end end
            self:Remove()
            return r
        end
        function NB:FindSpots(spec)
            spec = spec or {}
            local pos = spec.pos or self:GetPos()
            local radius = spec.radius or 1000
            local out = {}
            local area = navmesh.GetNavArea(pos, 200)
            local areas = IsValid(area) and navmesh.Find(pos, radius, 100, 100) or {}
            for _, a in ipairs(areas) do
                local spots = spec.type == "hiding" and a:GetHidingSpots() or { a:GetCenter() }
                for _, p in ipairs(spots) do
                    local d = p:Distance(pos)
                    if d <= radius then out[#out + 1] = { vector = p, distance = d, area = a } end
                end
            end
            table.sort(out, function(a, b) return a.distance < b.distance end)
            return out
        end
        function NB:FindSpot(kind, spec) local spots = self:FindSpots(spec); if kind == "random" and #spots > 0 then return spots[math.random(#spots)].vector end if kind == "far" and #spots > 0 then return spots[#spots].vector end return spots[1] and spots[1].vector end
    end
    if SERVER then
        -- NEXTBOT:OnKilled / OnOtherKilled / OnTraceAttack from the server's NPC events
        events.on("npc.killed", function(p)
            local victim = Entity(p.npc or 0)
            local attacker = (p.attacker or 0) ~= 0 and Player(p.attacker) or NULL
            local inst = scripted_ents and scripted_ents.__instance and scripted_ents.__instance(p.npc or 0)
            local d = DamageInfo and DamageInfo() or nil
            if d then d:SetAttacker(attacker); d:SetDamage(p.damage or 0) end
            -- @hook NEXTBOT:OnKilled
            if inst and inst.OnKilled then inst:OnKilled(d) end
            -- @hook NEXTBOT:OnOtherKilled
            for _, e in ipairs(ents.GetAll()) do
                if e ~= victim and e.IsNextBot and e:IsNextBot() then
                    local i = scripted_ents.__instance(e:EntIndex())
                    if i and i.OnOtherKilled then i:OnOtherKilled(victim, d) end
                end
            end
        end)
        events.on("npc.damage", function(p)
            local inst = scripted_ents and scripted_ents.__instance and scripted_ents.__instance(p.npc or p.entity or 0)
            -- @hook NEXTBOT:OnTraceAttack @hook ENTITY:OnTraceAttack (any scripted instance hit)
            if inst and inst.OnTraceAttack and DamageInfo then
                local d = DamageInfo(); d:SetDamage(p.damage or 0); d:SetAttacker((p.attacker or 0) ~= 0 and Player(p.attacker) or NULL)
                inst:OnTraceAttack(d, p.dir and Vector(p.dir.x, p.dir.y, p.dir.z) or Vector(0, 0, 0), { HitPos = p.pos and Vector(p.pos.x, p.pos.y, p.pos.z) or Entity(p.npc or 0):GetPos(), HitGroup = p.hitgroup or 0 })
            end
        end)
    end
end

------------------------------------------------------------------------ client / menu leftovers: steamworks (offline registry), workshop file base, UGC publish window, video writer, motion sensor, gmsave / engine dupe + save, widgets, properties menu, sound hints, jit.util, misc
-- @struct DebugInfo @struct PathSegment @struct PhysEnvPerformanceSettings @struct PhysicsObjectSave @struct Sky3DParams @struct PolygonVertex @struct RenderCamData @struct RenderCaptureData @struct SoundHintData @struct UGCFileInfo @struct VideoData @struct LocalLight
do
    steamworks = steamworks or {}
    local subscribed, mount, votes = {}, {}, {}
    local function addon_list() local ok, l = pcall(function() return engine.GetAddons() end); return ok and l or {} end
    local function file_info(id)
        for _, a in ipairs(addon_list()) do if tostring(a.wsid) == tostring(id) then return { id = tonumber(id) or 0, title = a.title or "", description = "", owner = 0, created = 0, updated = a.updated or 0, banned = false, tags = a.tags or "", size = a.size or 0, fileid = tonumber(id) or 0, previewid = 0, previewurl = "", installed = a.downloaded == true, disabled = a.mounted == false, children = {}, error = nil } end end
        return { id = tonumber(id) or 0, title = "", description = "", owner = 0, created = 0, updated = 0, banned = false, tags = "", size = 0, fileid = tonumber(id) or 0, previewid = 0, previewurl = "", installed = false, disabled = false, children = {}, error = 9 }
    end
    function steamworks.FileInfo(id, cb) if cb then cb(file_info(id)) end end
    function steamworks.GetPlayerName(steamid) local lp = LocalPlayer and LocalPlayer(); if IsValid(lp) and (steamid == nil or steamid == lp:SteamID64()) then return lp:Nick() end return "" end
    function steamworks.RequestPlayerInfo(steamid, cb) if cb then cb(steamworks.GetPlayerName(steamid)) end end
    function steamworks.IsSubscribed(id) if subscribed[tostring(id)] ~= nil then return subscribed[tostring(id)] end for _, a in ipairs(addon_list()) do if tostring(a.wsid) == tostring(id) then return true end end return false end
    -- (un)subscribing runs the menu's subscription-update callbacks at once: the registry is local
    -- @hook GM:WorkshopSubscriptionsChanged @hook GM:WorkshopSubscriptionsProgress @hook GM:WorkshopSubscriptionsMessage
    local function subs_changed(id, on)
        local function run(n, ...) if hook and hook.Run then hook.Run(n, ...) end end
        run("WorkshopSubscriptionsMessage", on and ("Subscribed to " .. tostring(id)) or ("Unsubscribed from " .. tostring(id)))
        run("WorkshopSubscriptionsProgress", 1, 1)
        run("WorkshopSubscriptionsChanged")
    end
    function steamworks.Subscribe(id) if subscribed[tostring(id)] ~= true then subscribed[tostring(id)] = true subs_changed(id, true) end end
    function steamworks.Unsubscribe(id) if subscribed[tostring(id)] ~= false then subscribed[tostring(id)] = false subs_changed(id, false) end end
    function steamworks.SetShouldMountAddon(id, on) mount[tostring(id)] = on ~= false end
    function steamworks.ShouldMountAddon(id) local m = mount[tostring(id)]; if m ~= nil then return m end for _, a in ipairs(addon_list()) do if tostring(a.wsid) == tostring(id) then return a.mounted ~= false end end return false end
    function steamworks.ApplyAddons() end
    function steamworks.GetList(kind, tags, offset, num, days, userid, cb) local results = {}; for _, a in ipairs(addon_list()) do results[#results + 1] = tonumber(a.wsid) or 0 end if cb then cb({ results = results, totalresults = #results, numresults = #results }) end end
    -- downloads resolve against the local addon registry (no Workshop connection): the menu's
    -- Workshop* callbacks fire around the lookup in the game's order
    -- @hook GM:WorkshopStart @hook GM:WorkshopDownloadTotals @hook GM:WorkshopDownloadFile @hook GM:WorkshopDownloadProgress @hook GM:WorkshopDownloadedFile @hook GM:WorkshopExtractProgress @hook GM:WorkshopEnd
    local function local_file(id) for _, a in ipairs(addon_list()) do if tostring(a.wsid) == tostring(id) then return a.file end end return nil end
    local function ws_hooks(id, path)
        local function run(n, ...) if hook and hook.Run then hook.Run(n, ...) end end
        run("WorkshopStart"); run("WorkshopDownloadTotals", 1, 1); run("WorkshopDownloadFile", id, 1, 1, "")
        run("WorkshopDownloadProgress", id, 1, 1, path and 1 or 0, 1); run("WorkshopDownloadedFile", id, path or "")
        run("WorkshopExtractProgress", id, 1, 1, 1); run("WorkshopEnd")
    end
    function steamworks.Download(previewid, uncompress, cb) local path = local_file(previewid); ws_hooks(previewid, path); if cb then cb(path) end end
    function steamworks.DownloadUGC(id, cb) local path = local_file(id); ws_hooks(id, path); if cb then cb(path, path and file and file.Open and file.Open(path, "rb", "GAME") or nil) end end
    function steamworks.OpenWorkshop() if gui and gui.OpenURL then gui.OpenURL("https://steamcommunity.com/app/4000/workshop/") end end
    function steamworks.ViewFile(id) if gui and gui.OpenURL then gui.OpenURL("https://steamcommunity.com/sharedfiles/filedetails/?id=" .. tostring(id)) end end
    function steamworks.Publish(image, gma, title, desc, tags, cb) if cb then cb(0, "Publishing needs a Workshop connection") end end
    function steamworks.Vote(id, up) votes[tostring(id)] = up == true end
    function steamworks.VoteInfo(id, cb) if cb then cb({ up = 0, down = 0, total = 0, score = 0, voted = votes[tostring(id)] ~= nil, votedUp = votes[tostring(id)] == true }) end end
    function steamworks.SetFileCompleted(id) end
    function steamworks.SetFilePlayed(id) end
    local WFB = __glua_classes.WorkshopFileBase
    if WFB then
        function WFB:Fetch(kind, offset, num, cb) steamworks.GetList(kind, self.Tags or {}, offset, num, 365, 0, function(data) if cb then cb(data) end end) end
        function WFB:FetchSubscribed(kind, offset, num, cb, extra) local results = {}; for _, a in ipairs(addon_list()) do if a.mounted ~= false or extra then results[#results + 1] = tonumber(a.wsid) or 0 end end if cb then cb({ results = results, totalresults = #results, numresults = #results }) end end
        function WFB:FillFileInfo(data) for k, v in pairs(file_info(data.id)) do if data[k] == nil then data[k] = v end end return data end
        function WFB:Publish(filename, image) steamworks.Publish(image, filename, self.Title or "", self.Description or "", self.Tags or {}, function(id, err) if self.OnPublishFinished then self:OnPublishFinished(id, err) end end) end
        function WFB:RetrieveUserName(steamid, cb) steamworks.RequestPlayerInfo(steamid, cb) end
    end
    local UGC
    do local ok, t = pcall(function() return derma and derma.GetControlTable and derma.GetControlTable("UGCPublishWindow") end); if ok then UGC = t end end
    if UGC then
        function UGC:Setup(kind, filename, image, info) self.Type, self.Filename, self.Image, self.Info = kind, filename, image, info or {} end
        function UGC:FitContents() if self.SizeToChildren then self:SizeToChildren(false, true) end end
        function UGC:GetChosenTag() return self.ChosenTag or "" end
        function UGC:CheckInput() if not self.Info or (self.Info.title or "") == "" then self:DisplayError("#ugc.title_required") return false end return true end
        function UGC:DisplayError(err) self.Error = err; if self.ErrorLabel and self.ErrorLabel.SetText then self.ErrorLabel:SetText(language and language.GetPhrase and language.GetPhrase(err) or err) end end
        function UGC:DoPublish() if not self:CheckInput() then return end steamworks.Publish(self.Image, self.Filename, self.Info.title or "", self.Info.description or "", { self:GetChosenTag() }, function(id, err) self:OnPublishFinished(id, err) end) end
        function UGC:OnPublishFinished(id, err) if err and err ~= "" then self:DisplayError(err) end end
        function UGC:UpdateWorkshopItems() end
    end
    -- video: a writer that counts frames (no encoder in this runtime) and leaves a .json manifest next to the videos
    local VW = {}
    VW.__index = VW
    __glua_classes.IVideoWriter = VW
    local function vw_new(cfg) return setmetatable({ cfg = cfg or {}, frames = 0, sound = false, finished = false }, VW) end
    function VW:AddFrame(dt, finalize) if self.finished then return end self.frames = self.frames + 1; self.time = (self.time or 0) + (dt or 0) end
    function VW:SetRecordSound(on) self.sound = on == true end
    function VW:Width() return self.cfg.width or ScrW() end
    function VW:Height() return self.cfg.height or ScrH() end
    function VW:Finish()
        self.finished = true
        if file and file.Write and util and util.TableToJSON then file.Write("videos/" .. (self.cfg.name or "video") .. ".json", util.TableToJSON({ frames = self.frames, time = self.time or 0, fps = self.cfg.fps or 30, width = self:Width(), height = self:Height(), container = self.cfg.container or "webm", video = self.cfg.video or "vp8", audio = self.cfg.audio or "vorbis" })) end
    end
    video = video or {}
    local active_writer
    function video.Record(cfg) local w = vw_new(cfg); active_writer = w; return w, nil end
    menu = menu or {}
    function menu.RecordFrame() if active_writer and not active_writer.finished then active_writer:AddFrame(FrameTime and FrameTime() or 1 / 30) end end
    motionsensor = motionsensor or {}
    function motionsensor.ProcessAngle(translator, sensor, pos, ang, special_vectors, boneid, v) return false end
    function motionsensor.ProcessAnglesTable(translator, sensor, pos, rotation) return {} end
    function motionsensor.ProcessPositionTable(translator, sensor) return {} end
    function motionsensor.BuildSkeleton(translator, ply, rotation) return {}, false end
    function motionsensor.ChooseBuilderFromEntity(ent) return IsValid(ent) and ent:IsPlayer() and "player" or "ragdoll" end
    hammer = hammer or {}
    function hammer.SendCommand(cmd) return false end
    -- gmsave: a save is the JSON of every saveable entity (model, pose, colour, material, frozen), players keep their pose
    gmsave = gmsave or {}
    function gmsave.ShouldSaveEntity(ent, t)
        if not IsValid(ent) or ent:IsPlayer() or ent:IsWeapon() or ent:IsWorld() then return false end
        if t and t.Class and (t.Class == "gmod_hands" or t.Class == "viewmodel") then return false end
        return ent.CreatedByMap == nil or not ent:CreatedByMap()
    end
    function gmsave.SaveMap(ply)
        local out = {}
        for _, e in ipairs(ents.GetAll()) do
            local t = { Class = e:GetClass() }
            if gmsave.ShouldSaveEntity(e, t) then
                local c = e:GetColor()
                out[#out + 1] = { Class = e:GetClass(), Model = e:GetModel(), Pos = e:GetPos(), Angle = e:GetAngles(), Color = { r = c.r, g = c.g, b = c.b, a = c.a }, Material = e:GetMaterial(), Skin = e:GetSkin(), Frozen = IsValid(e:GetPhysicsObject()) and not e:GetPhysicsObject():IsMotionEnabled() }
            end
        end
        return util.TableToJSON({ Entities = out, Map = game.GetMap(), Players = { IsValid(ply) and gmsave.PlayerSave(ply) or nil } })
    end
    function gmsave.LoadMap(str, ply, cb)
        local data = util.JSONToTable(str or "")
        if not data then return false end
        for _, t in ipairs(data.Entities or {}) do
            local e = ents.Create(t.Class or "prop_physics")
            if IsValid(e) then
                if t.Model then e:SetModel(t.Model) end
                if t.Pos then e:SetPos(Vector(t.Pos.x, t.Pos.y, t.Pos.z)) end
                if t.Angle then e:SetAngles(Angle(t.Angle.p or t.Angle.x or 0, t.Angle.y or 0, t.Angle.r or t.Angle.z or 0)) end
                e:Spawn()
                if t.Color then e:SetColor(Color(t.Color.r, t.Color.g, t.Color.b, t.Color.a)) end
                if t.Material and t.Material ~= "" then e:SetMaterial(t.Material) end
                if t.Skin then e:SetSkin(t.Skin) end
                if t.Frozen then local p = e:GetPhysicsObject(); if IsValid(p) then p:EnableMotion(false) end end
            end
        end
        if cb then cb() end
        return true
    end
    function gmsave.PlayerSave(ply) return { Pos = ply:GetPos(), Ang = ply:EyeAngles(), Health = ply:Health(), Armor = ply:Armor(), Model = ply:GetModel() } end
    function gmsave.PlayerLoad(ply, t)
        if not t then return end
        if t.Pos then ply:SetPos(Vector(t.Pos.x, t.Pos.y, t.Pos.z)) end
        if t.Ang then ply:SetEyeAngles(Angle(t.Ang.p or t.Ang.x or 0, t.Ang.y or 0, t.Ang.r or t.Ang.z or 0)) end
        if t.Health then ply:SetHealth(t.Health) end
        if t.Armor then ply:SetArmor(t.Armor) end
    end
    engine = engine or {}
    -- dupe / save containers: a JSON body with a header (the game's .dupe / .gms wrap the same fields)
    function engine.WriteDupe(json, png, name, desc, map) return util.TableToJSON({ type = "dupe", name = name or "", description = desc or "", map = map or (game and game.GetMap and game.GetMap()) or "", time = os.time(), data = json or "", preview = png ~= nil and png ~= "" }) end
    function engine.OpenDupe(str) local t = util.JSONToTable(str or ""); if not t or t.type ~= "dupe" then return nil end return t.data end
    function engine.WriteSave(json, name, time, map, workshopid) return util.TableToJSON({ type = "save", name = name or "", time = time or os.time(), map = map or "", workshopid = workshopid or "", data = json or "" }) end
    widgets = widgets or {}
    function widgets.PlayerTick(ply, mv)
        if not IsValid(ply) then return end
        local tr = ply:GetEyeTrace()
        local hovered = tr and tr.Entity
        if IsValid(hovered) and hovered.IsWidget then ply:SetHoveredWidget(hovered) else ply:SetHoveredWidget(NULL) end
        if ply:KeyPressed(IN_ATTACK) and IsValid(ply:GetHoveredWidget()) then ply:SetPressedWidget(ply:GetHoveredWidget()) end
        if ply:KeyReleased(IN_ATTACK) then ply:SetPressedWidget(NULL) end
    end
    function widgets.RenderMe(ent) if IsValid(ent) and ent.Draw and not ent.__widget_drawn then ent:Draw() end end
    hook.Add("PlayerTick", "__widgets", function(ply, mv) widgets.PlayerTick(ply, mv) end)
    properties = properties or {}
    function properties.OpenEntityMenu(ent, tr)
        if not IsValid(ent) or not DermaMenu then return end
        local menu = DermaMenu()
        for _, item in ipairs(properties.List and properties.GetList and properties.GetList() or {}) do
            local ok, show = pcall(item.Filter, item, ent, LocalPlayer())
            if ok and show then
                local opt = menu:AddOption(language and language.GetPhrase and language.GetPhrase(item.MenuLabel or "") or (item.MenuLabel or ""), function() if item.Action then item:Action(ent, tr) end end)
                if item.MenuIcon and opt.SetImage then opt:SetImage(item.MenuIcon) end
            end
        end
        menu:Open()
        return menu
    end
    function properties.OnScreenClick(eyepos, eyevec)
        local tr = util.TraceLine({ start = eyepos, endpos = eyepos + eyevec * 1024, filter = LocalPlayer and LocalPlayer() or nil })
        if not tr or not IsValid(tr.Entity) then return end
        properties.OpenEntityMenu(tr.Entity, tr)
    end
    sound = sound or {}
    local overrides, gender = {}, {}
    function sound.AddSoundOverrides(path) overrides[#overrides + 1] = path end
    function sound.SetActorGender(actor, g) gender[actor] = g end
    function sound.GetLoudestSoundHint(kind, pos) return nil end
    jit = jit or {}
    jit.util = jit.util or {}
    jit.opt = jit.opt or {}
    function jit.opt.start() end
    for _, n in ipairs({ "funcbc", "funcinfo", "funck", "funcuvname", "ircalladdr", "traceexitstub", "traceinfo", "traceir", "tracek", "tracemc", "tracesnap" }) do jit.util[n] = function() return nil end end
    function jit.util.funcinfo(f) local info = debug.getinfo(f, "S"); return info and { source = info.source, linedefined = info.linedefined, currentline = info.currentline, loc = info.short_src .. ":" .. tostring(info.linedefined) } or {} end
    -- function environments do not exist in Luau: setfenv keeps the function and remembers the table for getfenv
    local fenvs = setmetatable({}, { __mode = "k" })
    function setfenv(f, env) if type(f) == "function" then fenvs[f] = env end return f end
    if not rawget(_G, "__glua_getfenv_wrapped") then
        rawset(_G, "__glua_getfenv_wrapped", true)
        local base_getfenv = getfenv
        getfenv = function(f) if type(f) == "function" and fenvs[f] then return fenvs[f] end if base_getfenv then return base_getfenv(f) end return _G end
    end
    JS_Language = JS_Language or { GetPhrase = function(k) return language and language.GetPhrase and language.GetPhrase(k) or k end }
    JS_Utility = JS_Utility or { OpenURL = function(u) if gui and gui.OpenURL then gui.OpenURL(u) end end }
    JS_Workshop = JS_Workshop or { Subscribe = function(id) steamworks.Subscribe(id) end, Unsubscribe = function(id) steamworks.Unsubscribe(id) end, Vote = function(id, up) steamworks.Vote(id, up) end }
    function RenderDoF(origin, angle, focus, focusDist, blurStrength, blurQuality, viewScale, drawEntities) if DrawBokehDOF then DrawBokehDOF(blurStrength or 4, blurQuality or 1, focusDist or 256) end end
    function RenderSuperDoF(viewOrigin, viewAngles, viewFOV) if DrawBokehDOF then DrawBokehDOF(4, 1, 256) end end
    function RenderStereoscopy(viewOrigin, viewAngles) end
    function VisualizeLayout(panel) if panel and panel.InvalidateLayout then panel:InvalidateLayout(true) end end
    -- HTML / DHTML: console messages and target url changes reach the panel callbacks
    local DHTML
    do local ok, t = pcall(function() return (derma and derma.GetControlTable and derma.GetControlTable("DHTML")) or (vgui and vgui.GetControlTable and vgui.GetControlTable("DHTML")) end); if ok then DHTML = t end end
    if DHTML then
        local base_open = DHTML.OpenURL
        function DHTML:OpenURL(url)
            -- @hook HTML:OnChangeTargetURL
            if self.OnChangeTargetURL then self:OnChangeTargetURL(url) end
            if base_open then return base_open(self, url) end
        end
        function DHTML:__console(msg) if self.ConsoleMessage then self:ConsoleMessage(msg) end end -- @hook HTML:ConsoleMessage
        function DHTML:__child_view(url) if self.OnChildViewCreated then self:OnChildViewCreated(url) end end -- @hook HTML:OnChildViewCreated
        local base_run = DHTML.RunJavascript
        function DHTML:RunJavascript(js)
            if type(js) == "string" and string.find(js, "console.log(", 1, true) then local m = string.match(js, "console%.log%(%s*[\"'](.-)[\"']%s*%)"); if m then self:__console(m) end end
            if type(js) == "string" and string.find(js, "window.open(", 1, true) then local m = string.match(js, "window%.open%(%s*[\"'](.-)[\"']"); if m then self:__child_view(m) end end
            if base_run then return base_run(self, js) end
        end
    end
    -- tracers made with util.Effect ask the effect for its shoot position
    if util and util.Effect and effects and effects.__registry then
        local base_effect = util.Effect
        function util.Effect(name, data, predicted, recipients)
            local def = effects.__registry[name]
            if def and def.GetTracerShootPos and data and data.GetEntity then
                local ent = data:GetEntity()
                -- @hook EFFECT:GetTracerShootPos
                local pos = def.GetTracerShootPos(def, data:GetStart(), ent, data:GetAttachment())
                if pos and data.SetStart then data:SetStart(pos) end
            end
            return base_effect(name, data, predicted, recipients)
        end
    end
end

-- ---------------------------------------------------------------- ai.*
if SERVER then
    ai = ai or {}
    function ai.GetScheduleID(name) local v = _G[name]; if type(v) == "number" then return v end return -1 end
    function ai.GetTaskID(name) local v = _G[name]; if type(v) == "number" and string.sub(name, 1, 5) == "TASK_" then return v end return -1 end
    function ai.GetNodeCount() if navmesh and navmesh.GetNavAreaCount then return navmesh.GetNavAreaCount() end return 0 end
    local function squad_members(name)
        local out = {}
        for _, e in ipairs(ents.FindByClass("npc_*")) do if e.GetSquad and e:GetSquad() == name then out[#out + 1] = e end end
        return out
    end
    function ai.GetSquadMembers(name) return squad_members(name) end
    function ai.GetSquadMemberCount(name) return #squad_members(name) end
    function ai.GetSquadLeader(name) local m = squad_members(name); return m[1] or NULL end
end
