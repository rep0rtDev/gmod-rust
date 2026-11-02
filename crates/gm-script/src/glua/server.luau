-- GLua server-realm libraries over the host API: net (own transport, typed stream in a table),
-- ConVar / cvars (snapshot cvars + cvar ops), concommand (actor registry), file (addon storage),
-- CTakeDamageInfo, sound, undo / cleanup / constraint, and the GM hook bridge: every own server
-- event and request raises the documented gamemode hook with its documented arguments.
local stub = __glua_stub

------------------------------------------------------------------------ net
local hostnet = net or {}
net = {}
for k, v in pairs(hostnet) do net[k] = v end   -- the host's send / receive stay reachable
local out = nil           -- the message being written: { name = , items = { {t, v}, ... } }
local inbox = nil         -- the message being read: items + cursor
local strings = {}
local receivers = {}
local function wr(t, v)
    if not out then error("net.Write" .. t .. ": no net.Start") end
    out.items[#out.items + 1] = { t, v }
end
local function rd(t)
    if not inbox then return nil end
    inbox.pos += 1
    local item = inbox.items[inbox.pos]
    if not item then return nil end
    return item[2], item[1]
end
function net.Start(name, unreliable)
    if not strings[name] then strings[name] = true end
    out = { name = name, items = {} }
    return true
end
function net.Abort() out = nil end
function net.WriteString(s) wr("string", tostring(s)) end
function net.WriteInt(n, bits) wr("int", math.floor(n)) end
function net.WriteUInt(n, bits) wr("uint", math.max(0, math.floor(n))) end
function net.WriteUInt64(n) wr("uint64", tostring(n)) end
function net.WriteFloat(n) wr("float", n) end
function net.WriteDouble(n) wr("double", n) end
function net.WriteBool(b) wr("bool", b == true) end
net.WriteBit = net.WriteBool
function net.WriteVector(v) wr("vector", { x = v.x, y = v.y, z = v.z }) end
function net.WriteNormal(v) wr("normal", { x = v.x, y = v.y, z = v.z }) end
function net.WriteAngle(a) wr("angle", { p = a.p, y = a.y, r = a.r }) end
function net.WriteColor(c, alpha) wr("color", { r = c.r, g = c.g, b = c.b, a = alpha == false and 255 or c.a }) end
function net.WriteEntity(e) wr("entity", IsValid(e) and e:EntIndex() or 0) end
function net.WritePlayer(p) wr("player", IsValid(p) and p:UserID() or 0) end
function net.WriteData(s, len) wr("data", len and s:sub(1, len) or s) end
function net.WriteMatrix(m) wr("matrix", m and m:ToTable() or {}) end
function net.WriteTable(t, seq) wr("table", util.TableToJSON(t)) end
function net.WriteType(v)
    if isvector(v) then net.WriteVector(v) elseif isangle(v) then net.WriteAngle(v) elseif IsColor(v) then net.WriteColor(v)
    elseif IsEntity(v) then net.WriteEntity(v) elseif type(v) == "table" then net.WriteTable(v)
    elseif type(v) == "number" then net.WriteDouble(v) elseif type(v) == "boolean" then net.WriteBool(v)
    elseif v == nil then wr("nil", false) else net.WriteString(tostring(v)) end
end
function net.ReadString() local v = rd(); return v ~= nil and tostring(v) or "" end
function net.ReadInt() local v = rd(); return tonumber(v) or 0 end
net.ReadUInt = net.ReadInt
function net.ReadUInt64() local v = rd(); return tostring(v or "0") end
net.ReadFloat = net.ReadInt
net.ReadDouble = net.ReadInt
function net.ReadBool() local v = rd(); return v == true end
net.ReadBit = function() return net.ReadBool() and 1 or 0 end
function net.ReadVector() local v = rd(); return type(v) == "table" and Vector(v.x, v.y, v.z) or Vector() end
net.ReadNormal = net.ReadVector
function net.ReadAngle() local v = rd(); return type(v) == "table" and Angle(v.p, v.y, v.r) or Angle() end
function net.ReadColor() local v = rd(); return type(v) == "table" and Color(v.r, v.g, v.b, v.a) or Color(255, 255, 255) end
function net.ReadEntity() local v = rd(); return Entity(tonumber(v) or 0) end
function net.ReadPlayer() local v = rd(); return Player(tonumber(v) or 0) end
function net.ReadData() local v = rd(); return v ~= nil and tostring(v) or "" end
function net.ReadMatrix() local v = rd(); return v end
function net.ReadTable() local v = rd(); return type(v) == "string" and util.JSONToTable(v) or {} end
function net.ReadType()
    local v, t = rd()
    if t == "vector" or t == "normal" then return Vector(v.x, v.y, v.z) elseif t == "angle" then return Angle(v.p, v.y, v.r)
    elseif t == "color" then return Color(v.r, v.g, v.b, v.a) elseif t == "entity" then return Entity(v) elseif t == "player" then return Player(v)
    elseif t == "table" then return util.JSONToTable(v) elseif t == "nil" then return nil end
    return v
end
function net.ReadHeader() return 0 end
function net.BytesWritten() if not out then return 0 end return #util.TableToJSON(out.items) end
function net.BytesLeft() if not inbox then return 0, 0 end local n = #inbox.items - inbox.pos; return n * 8, n * 64 end
local function flush(target)
    if not out then error("net.Send: no net.Start") end
    hostnet.send(out.name, out.items, target)
    out = nil
end
function net.Send(target)
    if type(target) == "table" and target.IsValid == nil then
        for _, p in ipairs(target) do if IsValid(p) then hostnet.send(out.name, out.items, p:UserID()) end end
        out = nil
        return
    end
    if type(target) == "table" and target.GetPlayers then
        for _, p in ipairs(target:GetPlayers()) do hostnet.send(out.name, out.items, p:UserID()) end
        out = nil
        return
    end
    flush(IsValid(target) and target:UserID() or 0)
end
function net.Broadcast() flush(0) end
function net.SendToServer() flush(0) end
function net.SendOmit(omit)
    local skip = {}
    if type(omit) == "table" and omit.IsValid then skip[omit:UserID()] = true
    elseif type(omit) == "table" then for _, p in ipairs(omit) do skip[p:UserID()] = true end end
    for _, p in ipairs(player.GetAll()) do if not skip[p:UserID()] then hostnet.send(out.name, out.items, p:UserID()) end end
    out = nil
end
net.SendPVS = function() net.Broadcast() end
net.SendPAS = function() net.Broadcast() end
function net.Receive(name, fn) receivers[name] = fn end
function net.Incoming(len, ply)
    local name = net.ReadString()
    local fn = receivers[name]
    if fn then fn(len, ply) end
end
if events then
    events.on("net.message", function(p)
        local fn = receivers[p.name]
        if fn then
            inbox = { items = type(p.data) == "table" and p.data or {}, pos = 0 }
            fn(#inbox.items * 8, p.player and Player(p.player) or NULL)
            inbox = nil
        end
    end)
end
function util.AddNetworkString(name) strings[name] = true; return util.CRC(name) end
function util.NetworkStringToID(name) return strings[name] and tonumber(util.CRC(name)) or 0 end
function util.NetworkIDToString(id) for n in pairs(strings) do if tonumber(util.CRC(n)) == id then return n end end return nil end
__glua_classes.CRecipientFilter = nil
RecipientFilter = function()
    local R = {}
    R.__index = R
    local f = setmetatable({ list = {} }, R)
    function R:AddPlayer(p) self.list[p:UserID()] = p end
    function R:RemovePlayer(p) self.list[p:UserID()] = nil end
    function R:AddAllPlayers() for _, p in ipairs(player.GetAll()) do self.list[p:UserID()] = p end end
    function R:RemoveAllPlayers() self.list = {} end
    function R:GetPlayers() local o = {}; for _, p in pairs(self.list) do o[#o + 1] = p end return o end
    function R:GetCount() return table.Count(self.list) end
    function R:AddPAS() self:AddAllPlayers() end
    function R:AddPVS() self:AddAllPlayers() end
    function R:RemovePAS() end
    function R:RemovePVS() end
    function R:AddRecipientsByTeam(t) for _, p in ipairs(player.GetAll()) do if p:Team() == t then self.list[p:UserID()] = p end end end
    function R:RemoveRecipientsByTeam(t) for _, p in ipairs(player.GetAll()) do if p:Team() == t then self.list[p:UserID()] = nil end end end
    function R:RemoveRecipientsNotOnTeam(t) for id, p in pairs(self.list) do if p:Team() ~= t then self.list[id] = nil end end end
    function R:AddRecipientsByPVS() self:AddAllPlayers() end
    function R:RemoveMismatchedPlayers() end
    __glua_classes.CRecipientFilter = R
    return f
end
RecipientFilter()

------------------------------------------------------------------------ ConVar / cvars
local hostcvar = cvar
local CVAR = {}
CVAR.__index = CVAR
local convars = {}
local defaults, helps, flagsof, mins, maxs = {}, {}, {}, {}, {}
local function cvar_wrap(name)
    local c = convars[name]
    if c then return c end
    c = setmetatable({ __name = name }, CVAR)
    convars[name] = c
    return c
end
function CVAR:GetName() return self.__name end
function CVAR:GetString() local v = hostcvar and hostcvar.get(self.__name); if v == nil then return defaults[self.__name] or "" end return v end
function CVAR:GetInt() return math.floor(tonumber(self:GetString()) or 0) end
function CVAR:GetFloat() return tonumber(self:GetString()) or 0 end
function CVAR:GetBool() return (tonumber(self:GetString()) or 0) ~= 0 end
function CVAR:GetDefault() return defaults[self.__name] or "" end
function CVAR:GetHelpText() return helps[self.__name] or "" end
function CVAR:GetMin() return mins[self.__name] end
function CVAR:GetMax() return maxs[self.__name] end
function CVAR:GetFlags() return flagsof[self.__name] or 0 end
function CVAR:IsFlagSet(f) return bit.band(self:GetFlags(), f) ~= 0 end
function CVAR:SetString(v) hostcvar.set(self.__name, tostring(v)) end
function CVAR:SetInt(v) hostcvar.set(self.__name, tostring(math.floor(v))) end
function CVAR:SetFloat(v) hostcvar.set(self.__name, tostring(v)) end
function CVAR:SetBool(v) hostcvar.set(self.__name, v and "1" or "0") end
function CVAR:Revert() hostcvar.set(self.__name, defaults[self.__name] or "") end
__glua_classes.ConVar = CVAR
FCVAR_NONE, FCVAR_UNREGISTERED, FCVAR_DEVELOPMENTONLY, FCVAR_GAMEDLL, FCVAR_CLIENTDLL, FCVAR_HIDDEN, FCVAR_PROTECTED, FCVAR_SPONLY, FCVAR_ARCHIVE, FCVAR_NOTIFY, FCVAR_USERINFO, FCVAR_PRINTABLEONLY, FCVAR_UNLOGGED, FCVAR_NEVER_AS_STRING, FCVAR_REPLICATED, FCVAR_CHEAT, FCVAR_DEMO, FCVAR_DONTRECORD, FCVAR_NOT_CONNECTED, FCVAR_ARCHIVE_XBOX, FCVAR_SERVER_CAN_EXECUTE, FCVAR_SERVER_CANNOT_QUERY, FCVAR_CLIENTCMD_CAN_EXECUTE, FCVAR_LUA_CLIENT, FCVAR_LUA_SERVER = 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 65536, 131072, 4194304, 16777216, 268435456, 536870912, 1073741824, 262144, 524288
function CreateConVar(name, default, flags, help, min, max)
    local f = 0
    if type(flags) == "table" then for _, x in ipairs(flags) do f = bit.bor(f, x) end elseif type(flags) == "number" then f = flags end
    default = tostring(default)
    defaults[name], helps[name], flagsof[name], mins[name], maxs[name] = default, help, f, min, max
    if hostcvar and hostcvar.get(name) == nil then
        hostcvar.create(name, default, { replicated = bit.band(f, FCVAR_REPLICATED) ~= 0, archive = bit.band(f, FCVAR_ARCHIVE) ~= 0, help = help, min = min, max = max })
    end
    return cvar_wrap(name)
end
function CreateClientConVar(name, default, shouldsave, userinfo, help, min, max) return CreateConVar(name, default, 0, help, min, max) end
function GetConVar(name)
    if convars[name] then return convars[name] end
    if hostcvar and hostcvar.get(name) ~= nil then return cvar_wrap(name) end
    return nil
end
GetConVar_Internal = GetConVar
function GetConVarNumber(name) local c = GetConVar(name); return c and c:GetFloat() or 0 end
function GetConVarString(name) local c = GetConVar(name); return c and c:GetString() or "" end
function ConVarExists(name) return GetConVar(name) ~= nil end
cvars = {}
local cvar_callbacks = {}
function cvars.Number(name, default) local c = GetConVar(name); if not c then return default end return c:GetFloat() end
function cvars.String(name, default) local c = GetConVar(name); if not c then return default end return c:GetString() end
function cvars.Bool(name, default) local c = GetConVar(name); if not c then return default end return c:GetBool() end
function cvars.AddChangeCallback(name, fn, id)
    cvar_callbacks[name] = cvar_callbacks[name] or {}
    if id then cvar_callbacks[name][id] = fn else table.insert(cvar_callbacks[name], fn) end
end
function cvars.RemoveChangeCallback(name, id) if cvar_callbacks[name] then cvar_callbacks[name][id] = nil end end
function cvars.GetConVarCallbacks(name, createIfMissing)
    if not cvar_callbacks[name] and createIfMissing then cvar_callbacks[name] = {} end
    return cvar_callbacks[name]
end
function cvars.OnConVarChanged(name, old, new)
    local t = cvar_callbacks[name]
    if t then for _, fn in pairs(t) do fn(name, old, new) end end
end
if events then events.on("cvar.changed", function(p) cvars.OnConVarChanged(p.name, p.old, p.value) end) end

------------------------------------------------------------------------ concommand
concommand = {}
local commands = {}
local completes, helpsc = {}, {}
function concommand.Add(name, fn, autocomplete, help, flags) name = string.lower(name); commands[name] = fn; completes[name] = autocomplete; helpsc[name] = help end
function concommand.Remove(name) name = string.lower(name); commands[name] = nil; completes[name] = nil; helpsc[name] = nil end
function concommand.GetTable() return commands, completes, helpsc end
function concommand.Run(ply, cmd, args, argstr)
    local fn = commands[string.lower(cmd)]
    if not fn then return false end
    fn(ply or NULL, cmd, args or {}, argstr or table.concat(args or {}, " "))
    return true
end
function concommand.AutoComplete(cmd, args)
    local fn = completes[string.lower(cmd)]
    if fn then return fn(cmd, args) end
    return nil
end
function RunConsoleCommand(cmd, ...)
    local args = { ... }
    for i, a in ipairs(args) do args[i] = tostring(a) end
    if not concommand.Run(NULL, cmd, args, table.concat(args, " ")) and hostcvar and hostcvar.get(cmd) ~= nil then
        hostcvar.set(cmd, args[1] or "")
    end
end
function game.ConsoleCommand(cmd)
    local parts = string.Explode(" ", string.Trim(cmd))
    RunConsoleCommand(table.remove(parts, 1), unpack(parts))
end
function IsConCommandBlocked(cmd) return false end

------------------------------------------------------------------------ file (the addon's storage as its DATA folder)
file = {}
local function fkey(path) return "file:" .. string.lower(string.gsub(path, "\\", "/")) end
local function sget(k) if not storage then return nil end local ok, v = pcall(storage.get, k); if ok then return v end return nil end
local function sset(k, v) if not storage then return false end local ok = pcall(storage.set, k, v); return ok end
local function skeys() if not storage then return {} end local ok, v = pcall(storage.keys); if ok and type(v) == "table" then return v end return {} end
function file.Write(path, content) return sset(fkey(path), tostring(content)) end
function file.Append(path, content) local cur = sget(fkey(path)) or ""; return sset(fkey(path), cur .. tostring(content)) end
function file.Read(path, gamePath) local v = sget(fkey(path)); if v == nil then return nil end return tostring(v) end
function file.Exists(path, gamePath)
    if sget(fkey(path)) ~= nil then return true end
    return file.IsDir(path, gamePath)
end
function file.Delete(path, gamePath) return sset(fkey(path), nil) end
function file.CreateDir(path) return sset("dir:" .. string.lower(path), true) end
function file.IsDir(path, gamePath)
    if sget("dir:" .. string.lower(path)) then return true end
    local prefix = fkey(path) .. "/"
    for _, k in ipairs(skeys()) do if string.sub(k, 1, #prefix) == prefix then return true end end
    return false
end
function file.Size(path, gamePath) local v = sget(fkey(path)); return v and #tostring(v) or -1 end
function file.Time(path, gamePath) return 0 end
function file.Rename(from, to) local v = sget(fkey(from)); if v == nil then return false end sset(fkey(to), v); sset(fkey(from), nil); return true end
function file.Find(pattern, gamePath, sorting)
    local files, dirs = {}, {}
    local dir = string.GetPathFromFilename(pattern)
    local pat = "^" .. string.gsub(string.PatternSafe(string.lower(pattern)), "%%%*", "[^/]*") .. "$"
    local seen = {}
    for _, k in ipairs(skeys()) do
        if string.sub(k, 1, 5) == "file:" then
            local p = string.sub(k, 6)
            if string.find(p, pat) then files[#files + 1] = string.GetFileFromFilename(p)
            elseif string.sub(p, 1, #dir) == string.lower(dir) then
                local rest = string.sub(p, #dir + 1)
                local d = string.match(rest, "^([^/]+)/")
                if d and not seen[d] then seen[d] = true; dirs[#dirs + 1] = d end
            end
        elseif string.sub(k, 1, 4) == "dir:" then
            local p = string.sub(k, 5)
            if string.GetPathFromFilename(p .. "/x") == string.lower(dir) then local d = string.GetFileFromFilename(p); if not seen[d] then seen[d] = true; dirs[#dirs + 1] = d end end
        end
    end
    table.sort(files)
    table.sort(dirs)
    return files, dirs
end
function file.AsyncRead(path, gamePath, cb) local v = file.Read(path, gamePath); cb(path, gamePath, v and 0 or -1, v or ""); return 0 end
function file.AsyncWrite(path, content, cb) local ok = file.Write(path, content); if cb then cb(path, ok and 0 or -1) end return 0 end
function file.AsyncAppend(path, content, cb) local ok = file.Append(path, content); if cb then cb(path, ok and 0 or -1) end return 0 end
local FILE = {}
FILE.__index = FILE
function file.Open(path, mode, gamePath)
    local data = file.Read(path, gamePath)
    if mode == "r" or mode == "rb" then if data == nil then return nil end
    elseif mode == "w" or mode == "wb" then data = ""
    elseif mode == "a" or mode == "ab" then data = data or "" end
    local f = setmetatable({ path = path, data = data, pos = (mode == "a" or mode == "ab") and #data or 0, mode = mode }, FILE)
    return f
end
function FILE:Close() if self.mode ~= "r" and self.mode ~= "rb" then file.Write(self.path, self.data) end end
function FILE:Flush() self:Close() end
function FILE:Size() return #self.data end
function FILE:Tell() return self.pos end
function FILE:Seek(pos) self.pos = math.Clamp(pos, 0, #self.data); return self.pos end
function FILE:Skip(n) return self:Seek(self.pos + n) end
function FILE:EndOfFile() return self.pos >= #self.data end
function FILE:Read(n) n = n or (#self.data - self.pos); local s = string.sub(self.data, self.pos + 1, self.pos + n); self.pos += #s; return s end
function FILE:ReadLine()
    if self:EndOfFile() then return nil end
    local nl = string.find(self.data, "\n", self.pos + 1, true)
    local s
    if nl then s = string.sub(self.data, self.pos + 1, nl) else s = string.sub(self.data, self.pos + 1) end
    self.pos += #s
    return s
end
function FILE:Write(s) s = tostring(s); self.data = string.sub(self.data, 1, self.pos) .. s .. string.sub(self.data, self.pos + #s + 1); self.pos += #s end
function FILE:WriteString(s) self:Write(s) end
function FILE:ReadString() return self:Read() end
function FILE:ReadBool() return self:Read(1) ~= "\0" end
function FILE:WriteBool(b) self:Write(b and "\1" or "\0") end
function FILE:ReadByte() local s = self:Read(1); return s ~= "" and string.byte(s) or 0 end
function FILE:WriteByte(b) self:Write(string.char(bit.band(b, 255))) end
function FILE:ReadLong() return (tonumber(self:Read(11)) or 0) end
function FILE:WriteLong(n) self:Write(string.format("%11d", n)) end
function FILE:ReadShort() return (tonumber(self:Read(6)) or 0) end
function FILE:WriteShort(n) self:Write(string.format("%6d", n)) end
function FILE:ReadULong() return self:ReadLong() end
function FILE:WriteULong(n) self:WriteLong(n) end
function FILE:ReadUShort() return self:ReadShort() end
function FILE:WriteUShort(n) self:WriteShort(n) end
function FILE:ReadFloat() return (tonumber(self:Read(24)) or 0) end
function FILE:WriteFloat(n) self:Write(string.format("%24.9g", n)) end
function FILE:ReadDouble() return self:ReadFloat() end
function FILE:WriteDouble(n) self:WriteFloat(n) end
function FILE:ReadUInt64() return self:Read(20) end
function FILE:WriteUInt64(n) self:Write(string.format("%20s", tostring(n))) end
__glua_classes.File = FILE

------------------------------------------------------------------------ CTakeDamageInfo
local DMG = {}
DMG.__index = DMG
function DamageInfo()
    return setmetatable({ damage = 0, base = 0, attacker = NULL, inflictor = NULL, dtype = DMG_GENERIC, pos = Vector(), force = Vector(), reported = Vector(), ammo = -1, max = 0, bonus = 0, custom = 0 }, DMG)
end
function DMG:GetDamage() return self.damage end
function DMG:SetDamage(d) self.damage = d end
function DMG:AddDamage(d) self.damage += d end
function DMG:SubtractDamage(d) self.damage -= d end
function DMG:ScaleDamage(s) self.damage *= s end
function DMG:GetBaseDamage() return self.base end
function DMG:SetBaseDamage(d) self.base = d end
function DMG:GetMaxDamage() return self.max end
function DMG:SetMaxDamage(d) self.max = d end
function DMG:GetAttacker() return self.attacker end
function DMG:SetAttacker(e) self.attacker = e end
function DMG:GetInflictor() return self.inflictor end
function DMG:SetInflictor(e) self.inflictor = e end
function DMG:GetDamageType() return self.dtype end
function DMG:SetDamageType(t) self.dtype = t end
function DMG:IsDamageType(t) return bit.band(self.dtype, t) ~= 0 end
function DMG:IsBulletDamage() return self:IsDamageType(DMG_BULLET) end
function DMG:IsExplosionDamage() return self:IsDamageType(DMG_BLAST) end
function DMG:IsFallDamage() return self:IsDamageType(DMG_FALL) end
function DMG:GetDamagePosition() return self.pos end
function DMG:SetDamagePosition(v) self.pos = v end
function DMG:GetDamageForce() return self.force end
function DMG:SetDamageForce(v) self.force = v end
function DMG:GetReportedPosition() return self.reported end
function DMG:SetReportedPosition(v) self.reported = v end
function DMG:GetAmmoType() return self.ammo end
function DMG:SetAmmoType(a) self.ammo = a end
function DMG:GetDamageBonus() return self.bonus end
function DMG:SetDamageBonus(b) self.bonus = b end
function DMG:GetDamageCustom() return self.custom end
function DMG:SetDamageCustom(c) self.custom = c end
__glua_classes.CTakeDamageInfo = DMG
function PLYDamageInfoEntity(e) return e end

------------------------------------------------------------------------ sound
sound = sound or {}
local sound_table = {}
function sound.Add(t) sound_table[t.name] = t end
function sound.Get(name) return sound_table[name] end
function sound.GetTable() local o = {}; for n in pairs(sound_table) do o[#o + 1] = n end return o end
function sound.GetProperties(name) return sound_table[name] end
function sound.GetLoopingSounds() return {} end
function sound.Generate() end
function sound.EmitHint() end
function sound.Play(name, pos, level, pitch, volume, ent)
    local s = sound_table[name]
    local file_ = name
    if s and s.sound then file_ = type(s.sound) == "table" and s.sound[math.random(#s.sound)] or s.sound end
    world.queue("sound.play", 0, { name = file_, pos = { x = pos.x, y = pos.y, z = pos.z }, volume = volume or (s and s.volume) or 1,
        ent = ent or 0 })
end
function sound.PlayFile(path, flags, cb) if cb then cb(nil, 20, "BASS_ERROR_NOTAVAIL") end end
function sound.PlayURL(url, flags, cb) if cb then cb(nil, 20, "BASS_ERROR_NOTAVAIL") end end
function EmitSound(name, pos, ent, channel, volume, level, flags, pitch) sound.Play(name, pos, level, pitch, volume, ent or 0) end
local ENTm = __glua_classes.Entity
function ENTm:EmitSound(name, level, pitch, volume) sound.Play(name, self:GetPos(), level, pitch, volume, self:EntIndex()) end
function ENTm:StopSound(name) end
__glua_classes.Player.EmitSound = ENTm.EmitSound
__glua_classes.Player.StopSound = ENTm.StopSound
SNDLVL_NONE, SNDLVL_20dB, SNDLVL_25dB, SNDLVL_30dB, SNDLVL_35dB, SNDLVL_40dB, SNDLVL_45dB, SNDLVL_50dB, SNDLVL_55dB, SNDLVL_IDLE, SNDLVL_60dB, SNDLVL_65dB, SNDLVL_STATIC, SNDLVL_70dB, SNDLVL_NORM, SNDLVL_75dB, SNDLVL_80dB, SNDLVL_TALKING, SNDLVL_85dB, SNDLVL_90dB, SNDLVL_95dB, SNDLVL_100dB, SNDLVL_105dB, SNDLVL_110dB, SNDLVL_120dB, SNDLVL_130dB, SNDLVL_GUNFIRE, SNDLVL_140dB, SNDLVL_150dB, SNDLVL_180dB = 0, 20, 25, 30, 35, 40, 45, 50, 55, 60, 60, 65, 66, 70, 75, 75, 80, 80, 85, 90, 95, 100, 105, 110, 120, 130, 140, 140, 150, 180
CHAN_REPLACE, CHAN_AUTO, CHAN_WEAPON, CHAN_VOICE, CHAN_ITEM, CHAN_BODY, CHAN_STREAM, CHAN_STATIC, CHAN_VOICE2, CHAN_VOICE_BASE, CHAN_USER_BASE = -1, 0, 1, 2, 3, 4, 5, 6, 7, 8, 136

------------------------------------------------------------------------ undo / cleanup / constraint
undo = {}
local cur_undo = nil
local undo_table = {}
function undo.Create(name) cur_undo = { Name = name, Entities = {}, Functions = {}, Owner = NULL, CustomUndoText = nil } end
function undo.AddEntity(e) if cur_undo and IsValid(e) then table.insert(cur_undo.Entities, e) end end
function undo.AddFunction(fn, ...) if cur_undo then table.insert(cur_undo.Functions, { fn, { ... } }) end end
function undo.SetPlayer(p) if cur_undo then cur_undo.Owner = p end end
function undo.SetCustomUndoText(t) if cur_undo then cur_undo.CustomUndoText = t end end
function undo.ReplaceEntity(from, to) if cur_undo then for i, e in ipairs(cur_undo.Entities) do if e == from then cur_undo.Entities[i] = to return true end end end return false end
function undo.Finish(nicetext)
    if not cur_undo then return end
    local u = cur_undo
    cur_undo = nil
    local id = IsValid(u.Owner) and u.Owner:UserID() or 0
    undo_table[id] = undo_table[id] or {}
    table.insert(undo_table[id], u)
    local ents_ = {}
    for _, e in ipairs(u.Entities) do ents_[#ents_ + 1] = e:EntIndex() end
    if #ents_ > 0 then pcall(world.queue, "undo.push", id, { name = u.Name, text = u.CustomUndoText or nicetext or ("Undone " .. u.Name), entities = ents_ }) end
end
function undo.GetTable() return undo_table end
function undo.Do_Undo(u)
    for _, e in ipairs(u.Entities) do if IsValid(e) then e:Remove() end end
    for _, f in ipairs(u.Functions) do f[1](u, unpack(f[2])) end
    return #u.Entities
end
function undo.MakeUIDirty() end
function undo.SetupUI() end
function undo.AddUndo() end
function undo.GetUndoTable() return undo_table end
cleanup = {}
local cleanup_types = { "props", "ragdolls", "effects", "npcs", "vehicles", "constraints", "ropeconstraints", "sents", "lamps", "lights", "thrusters", "wheels", "buttons", "emitters", "dynamite", "balloons", "hoverballs", "ducts", "cameras" }
local cleanup_lists = {}
function cleanup.Register(t) table.insert(cleanup_types, t) end
function cleanup.GetTable() return cleanup_types end
function cleanup.Add(ply, t, e)
    if not IsValid(ply) or not IsValid(e) then return end
    local id = ply:UserID()
    cleanup_lists[id] = cleanup_lists[id] or {}
    cleanup_lists[id][t] = cleanup_lists[id][t] or {}
    table.insert(cleanup_lists[id][t], e)
end
function cleanup.GetList() return cleanup_lists end
function cleanup.ReplaceEntity(from, to)
    for _, kinds in pairs(cleanup_lists) do for _, list in pairs(kinds) do for i, e in ipairs(list) do if e == from then list[i] = to return true end end end end
    return false
end
function cleanup.CC_Cleanup(ply, cmd, args)
    local id = IsValid(ply) and ply:UserID() or 0
    local kinds = cleanup_lists[id]
    if not kinds then return end
    for t, list in pairs(kinds) do
        if not args or not args[1] or args[1] == t then for _, e in ipairs(list) do if IsValid(e) then e:Remove() end end kinds[t] = {} end
    end
end
function cleanup.CC_AdminCleanup(ply, cmd, args) for id in pairs(cleanup_lists) do cleanup.CC_Cleanup(Player(id), cmd, args) end end
function cleanup.UpdateUI() end
constraint = {}
local constraints = {}
local function remember(kind, e1, e2, data)
    local c = { Type = kind, Ent1 = e1, Ent2 = e2 }
    for k, v in pairs(data or {}) do c[k] = v end
    table.insert(constraints, c)
    return c
end
function constraint.Weld(ent1, ent2, bone1, bone2, forcelimit, nocollide, deleteOnBreak)
    local owner = IsValid(ent1) and ent1:GetOwner()
    world.queue("weld", ent1:EntIndex(), { ent2 = IsValid(ent2) and ent2:EntIndex() or 0, forcelimit = forcelimit or 0, nocollide = nocollide or false, owner = IsValid(owner) and owner:UserID() or 0 })
    return remember("Weld", ent1, ent2, { forcelimit = forcelimit, nocollide = nocollide }), nil
end
function constraint.NoCollide(ent1, ent2, bone1, bone2)
    local owner = IsValid(ent1) and ent1:GetOwner()
    world.queue("nocollide", ent1:EntIndex(), { ent2 = IsValid(ent2) and ent2:EntIndex() or 0, owner = IsValid(owner) and owner:UserID() or 0 })
    return remember("NoCollide", ent1, ent2)
end
function constraint.GetTable(ent) local o = {}; for _, c in ipairs(constraints) do if c.Ent1 == ent or c.Ent2 == ent then o[#o + 1] = c end end return o end
function constraint.HasConstraints(ent) return #constraint.GetTable(ent) > 0 end
function constraint.Find(ent1, ent2, kind) for _, c in ipairs(constraints) do if c.Type == kind and ((c.Ent1 == ent1 and c.Ent2 == ent2) or (c.Ent1 == ent2 and c.Ent2 == ent1)) then return c end end return nil end
function constraint.FindConstraint(ent, kind) for _, c in ipairs(constraints) do if c.Type == kind and (c.Ent1 == ent or c.Ent2 == ent) then return c end end return nil end
function constraint.FindConstraints(ent, kind) local o = {}; for _, c in ipairs(constraints) do if c.Type == kind and (c.Ent1 == ent or c.Ent2 == ent) then o[#o + 1] = c end end return o end
function constraint.FindConstraintEntity(ent, kind) local c = constraint.FindConstraint(ent, kind); return c and (c.Ent1 == ent and c.Ent2 or c.Ent1) or NULL end
function constraint.GetAllConstrainedEntities(ent, out)
    out = out or {}
    if not IsValid(ent) or out[ent] then return out end
    out[ent] = ent
    for _, c in ipairs(constraint.GetTable(ent)) do constraint.GetAllConstrainedEntities(c.Ent1 == ent and c.Ent2 or c.Ent1, out) end
    return out
end
function constraint.RemoveAll(ent) local n = 0; for i = #constraints, 1, -1 do local c = constraints[i]; if c.Ent1 == ent or c.Ent2 == ent then table.remove(constraints, i); n += 1 end end return n > 0, n end
function constraint.RemoveConstraints(ent, kind) local n = 0; for i = #constraints, 1, -1 do local c = constraints[i]; if c.Type == kind and (c.Ent1 == ent or c.Ent2 == ent) then table.remove(constraints, i); n += 1 end end return n > 0, n end
function constraint.CanConstrain(ent, bone) return IsValid(ent) end
function constraint.ForgetConstraints(ent) constraint.RemoveAll(ent) end
function constraint.AddConstraintTable() end
function constraint.AddConstraintTableNoDelete() end
function constraint.CreateKeyframeRope() return NULL end
function constraint.CreateStaticAnchorPoint(pos) return NULL end
for _, kind in ipairs({ "Axis", "AdvBallsocket", "Ballsocket", "Elastic", "Hydraulic", "Keepupright", "Motor", "Muscle", "Pulley", "Rope", "Slider", "Winch" }) do
    constraint[kind] = stub("constraint." .. kind)
end

------------------------------------------------------------------------ GM hook bridge
GM = GM or {}
GAMEMODE = GM
GM.Name = "Sandbox"
GM.Folder = "gamemodes/sandbox"
local function dmginfo_for(p)
    local d = DamageInfo()
    d:SetDamage(p.amount or 0)
    d:SetDamageType(p.type or DMG_GENERIC)
    if p.attacker and p.attacker ~= 0 then d:SetAttacker(Player(p.attacker)) end
    return d
end
IN_ATTACK, IN_JUMP, IN_DUCK, IN_FORWARD, IN_BACK, IN_USE, IN_CANCEL, IN_LEFT, IN_RIGHT, IN_MOVELEFT, IN_MOVERIGHT, IN_ATTACK2, IN_RUN, IN_RELOAD, IN_ALT1, IN_ALT2, IN_SCORE, IN_SPEED, IN_WALK, IN_ZOOM, IN_WEAPON1, IN_WEAPON2, IN_BULLRUSH, IN_GRENADE1, IN_GRENADE2 = 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608, 16777216
local function in_key(name) return _G[name] or 0 end
if events then
    local initialized = false
local seen_players = {}
local prev_move = {}
    events.on("tick.pre", function()
        if not initialized then
            initialized = true
            hook.Run("PreGamemodeLoaded")
            hook.Run("OnGamemodeLoaded")
            hook.Run("CreateTeams")
            hook.Run("Initialize")
            hook.Run("InitPostEntity")
            hook.Run("PostGamemodeLoaded")
        end
        local hooks = hook.GetTable()
        local function wanted(name) return hooks[name] ~= nil and next(hooks[name]) ~= nil end
        local per_tick = wanted("PlayerTick") or wanted("PlayerPostThink")
        local deltas = wanted("OnPlayerJump") or wanted("OnPlayerHitGround") or wanted("PlayerAmmoChanged")
        local first_sight = wanted("PlayerConnect") or wanted("PlayerAuthed") or wanted("NetworkIDValidated") or wanted("PlayerInitialSpawn") or wanted("PlayerSetModel")
        if not (per_tick or first_sight or deltas) then return end
        for _, ply in ipairs(player.GetAll()) do
            if first_sight and not seen_players[ply:UserID()] then
                seen_players[ply:UserID()] = true
                hook.Run("PlayerConnect", ply:Nick(), "")
                hook.Run("PlayerAuthed", ply, ply:SteamID(), ply:UniqueID())
                hook.Run("NetworkIDValidated", ply:Nick(), ply:SteamID())
                hook.Run("PlayerInitialSpawn", ply, false)
                hook.Run("PlayerSetModel", ply)
            end
            if per_tick then
                hook.Run("PlayerTick", ply, nil)
                hook.Run("PlayerPostThink", ply)
            end
            if deltas then
                local uid = ply:UserID()
                local pm = prev_move[uid]
                local on_ground, vel, ammo = ply:IsOnGround(), ply:GetVelocity(), ply:GetAmmo() or {}
                if pm then
                    if pm.on_ground and ply:KeyPressed(IN_JUMP) then hook.Run("OnPlayerJump", ply, vel:Length2D()) end
                    if on_ground and not pm.on_ground then hook.Run("OnPlayerHitGround", ply, false, false, -pm.vel.z) end
                    for id, n in pairs(ammo) do
                        if (pm.ammo[id] or 0) ~= n then hook.Run("PlayerAmmoChanged", ply, id, pm.ammo[id] or 0, n) end
                    end
                    for id, n in pairs(pm.ammo) do
                        if ammo[id] == nil and n ~= 0 then hook.Run("PlayerAmmoChanged", ply, id, n, 0) end
                    end
                end
                prev_move[uid] = { on_ground = on_ground, vel = vel, ammo = ammo }
            end
        end
    end, -1e9 + 1)
    events.on("addon.reloaded", function() hook.Run("OnReloaded") end)
    events.on("addon.reloaded", function() hook.Run("ShutDown") end, 1e9)
    events.on("entity.created", function(p) hook.Run("OnEntityCreated", Entity(p.entity)) end)
    events.on("entity.spawned", function(p) if not p.tag then local e = Entity(p.entity); hook.Run("PlayerSpawnedProp", Player(p.player), e:GetModel(), e) end end)
    events.on("entity.removed", function(p) hook.Run("EntityRemoved", Entity(p.entity), false) end)
    events.on("entity.break", function(p) hook.Run("PropBreak", NULL, Entity(p.entity)) end)
    events.on("npc.killed", function(p) hook.Run("OnNPCKilled", Entity(p.npc), p.attacker ~= 0 and Player(p.attacker) or NULL, NULL) end)
    events.on("player.damaged", function(p) local ply = Player(p.player); hook.Run("PlayerHurt", ply, p.attacker and p.attacker ~= 0 and Player(p.attacker) or NULL, ply:Health(), p.amount) end)
    events.on("player.death", function(p)
        local ply = Player(p.player)
        local attacker = p.attacker ~= 0 and Player(p.attacker) or NULL
        hook.Run("DoPlayerDeath", ply, attacker, dmginfo_for(p))
        hook.Run("PlayerDeath", ply, NULL, attacker)
        hook.Run("PostPlayerDeath", ply)
    end)
    events.on("player.noclip", function(p) hook.Run("PlayerNoClip", Player(p.player), p.on == 1) end)
    events.on("player.used", function(p) hook.Run("PlayerUsedEntity", Player(p.player), Entity(p.entity)) end)
    events.on("player.use_player", function(p) hook.Run("PlayerUse", Player(p.player), Player(p.target)) end)
    events.on("player.pickup", function(p) hook.Run("GravGunOnPickedUp", Player(p.player), Entity(p.entity)) end)
    events.on("player.drop", function(p) hook.Run("GravGunOnDropped", Player(p.player), Entity(p.entity)) end)
    events.on("player.weapon_pickup", function(p) hook.Run("WeaponEquip", __glua_classes.Weapon.__from(p.class, p.player), Player(p.player)); local i = weapons.__ensure(p.class, p.player); if i then rawset(i, "Owner", Player(p.player)); if i.Equip then i:Equip(Player(p.player)) end if i.OwnerChanged then i:OwnerChanged() end end end)
    events.on("player.enter_vehicle", function(p) hook.Run("PlayerEnteredVehicle", Player(p.player), Entity(p.vehicle), 0) end)
    events.on("player.exit_vehicle", function(p) hook.Run("PlayerLeaveVehicle", Player(p.player), Entity(p.vehicle)) end)
    events.on("player.key_press", function(p) hook.Run("KeyPress", Player(p.player), in_key(p.key)) end)
    events.on("player.key_release", function(p) hook.Run("KeyRelease", Player(p.player), in_key(p.key)) end)
    events.on("player.property", function(p) hook.Run("PropertyUsed", Player(p.player), p.property, Entity(p.entity)) end)
    events.on("player.unfroze", function(p) hook.Run("OnPhysgunReload", NULL, Player(p.player)); hook.Run("PlayerUnfrozeObject", Player(p.player), NULL, NULL) end)
    events.on("physgun.pickup", function(p) hook.Run("OnPhysgunPickup", Player(p.player), Entity(p.entity)); hook.Run("OnPlayerPhysicsPickup", Player(p.player), Entity(p.entity)) end)
    events.on("physgun.drop", function(p) hook.Run("PhysgunDrop", Player(p.player), Entity(p.entity)); hook.Run("OnPlayerPhysicsDrop", Player(p.player), Entity(p.entity), false) end)
    events.on("physgun.freeze", function(p) local e = Entity(p.entity); hook.Run("OnPhysgunFreeze", NULL, e:GetPhysicsObject(), e, Player(p.player)); hook.Run("PlayerFrozeObject", Player(p.player), e, e:GetPhysicsObject()) end)
    -- requests: the documented hook's cancel value becomes the own request's `false`
    events.on("player.damage", function(p)
        local ply = Player(p.player)
        local d = dmginfo_for(p)
        if hook.Run("PlayerShouldTakeDamage", ply, d:GetAttacker()) == false then return false end
        local group = p.group or HITGROUP_GENERIC
        if p.px then
            local pos = Vector(p.px, p.py, p.pz)
            d:SetDamagePosition(pos)
            if p.attacker and p.attacker ~= 0 then
                local dir = (pos - Player(p.attacker):EyePos()):GetNormalized()
                local tr = { Hit = true, HitPos = pos, HitGroup = group, Entity = ply, Fraction = 1, Normal = dir }
                if hook.Run("PlayerTraceAttack", ply, d, dir, tr) == true then return false end
            end
        end
        hook.Run("ScalePlayerDamage", ply, group, d)
        if hook.Run("EntityTakeDamage", ply, d) == true then return false end
        hook.Run("PostEntityTakeDamage", ply, d, true)
        if d:IsDamageType(DMG_BLAST) then hook.Run("OnDamagedByExplosion", ply, d) end
        if d:GetDamage() ~= (p.amount or 0) then return d:GetDamage() end
    end)
    events.on("player.fall_damage", function(p)
        local r = hook.Run("GetFallDamage", Player(p.player), p.speed)
        if isnumber(r) then return r end
    end)
    events.on("player.use", function(p) if hook.Run("PlayerUse", Player(p.player), Entity(p.entity)) == false then return false end end)
    events.on("player.pickup_request", function(p)
        local ply, e = Player(p.player), Entity(p.entity)
        if hook.Run("GravGunPickupAllowed", ply, e) == false then return false end
        if hook.Run("AllowPlayerPickup", ply, e) == false then return false end
    end)
    events.on("physgun.pickup_request", function(p) if hook.Run("PhysgunPickup", Player(p.player), Entity(p.entity)) == false then return false end end)
    events.on("player.unfreeze_request", function(p) local e = Entity(p.entity); if hook.Run("CanPlayerUnfreeze", Player(p.player), e, e:GetPhysicsObject()) == false then return false end end)
    events.on("player.undo_request", function(p)
        local u = { Name = p.name, Owner = Player(p.player) }
        if hook.Run("CanUndo", Player(p.player), u) == false then return false end
        hook.Run("PreUndo", u)
        hook.Run("OnUndo", Player(p.player), p.name, u.CustomUndoText or "")
        hook.Run("PostUndo", u, 1)
    end)
    events.on("player.property_request", function(p) if hook.Run("CanProperty", Player(p.player), p.property, Entity(p.entity)) == false then return false end end)
    events.on("player.pickup_weapon_request", function(p) if hook.Run("PlayerCanPickupWeapon", Player(p.player), __glua_classes.Weapon.__from(p.class, p.player)) == false then return false end end)
    events.on("player.enter_vehicle_request", function(p) if hook.Run("CanPlayerEnterVehicle", Player(p.player), Entity(p.vehicle), 0) == false then return false end end)
    events.on("player.exit_vehicle_request", function(p) if hook.Run("CanExitVehicle", Entity(p.vehicle), Player(p.player)) == false then return false end end)
    events.on("npc.damage", function(p)
        local d = dmginfo_for(p)
        d:SetDamageType(DMG_BULLET)
        hook.Run("ScaleNPCDamage", Entity(p.npc), p.group or HITGROUP_GENERIC, d)
        if hook.Run("EntityTakeDamage", Entity(p.npc), d) == true then return false end
        if d:GetDamage() ~= (p.amount or 0) then return d:GetDamage() end
    end)
    local function bullet_data(p)
        return { Attacker = Player(p.attacker), Src = Vector(p.sx, p.sy, p.sz), Dir = Vector(p.dx, p.dy, p.dz),
            Num = p.pellets, Damage = p.damage or 0, AmmoType = p.ammo, Spread = Vector(0, 0, 0), Tracer = 1, Force = 1 }
    end
    events.on("entity.fire_bullets_request", function(p)
        if hook.Run("EntityFireBullets", Player(p.attacker), bullet_data(p)) == false then return false end
    end)
    events.on("entity.water_level_changed", function(p) hook.Run("OnEntityWaterLevelChanged", Player(p.entity), p.old or 0, p.new or 0) end)
    events.on("entity.fired_bullets", function(p) hook.Run("PostEntityFireBullets", Player(p.attacker), bullet_data(p)) end)
    events.on("sandbox.check_limit", function(p)
        if hook.Run("PlayerCheckLimit", Player(p.player), p.kind, p.current or 0, p.max or -1) == false then return false end
    end)
    events.on("player.footstep", function(p)
        local filter = RecipientFilter and RecipientFilter() or nil
        if hook.Run("PlayerFootstep", Player(p.player), Vector(p.px, p.py, p.pz), p.right or 0, p.sound, p.volume or 1, filter) == true then
            return false
        end
    end)
    events.on("entity.emit_sound", function(p)
        local t = {
            SoundName = p.name, OriginalSoundName = p.name, Entity = Entity(p.entity),
            Pos = Vector(p.px, p.py, p.pz), Volume = p.volume or 1, Pitch = 100,
            SoundLevel = 75, DSP = 0, Channel = CHAN_AUTO or 0, Flags = 0,
        }
        local r = hook.Run("EntityEmitSound", t)
        if r == false then return false end
        if r == true then
            if t.SoundName ~= p.name then return t.SoundName end
            if t.Volume ~= p.volume then return t.Volume end
        end
    end)
    events.on("player.select_spawn", function(p)
        local r = hook.Run("PlayerSelectSpawn", Player(p.player), false)
        if r ~= nil and r ~= NULL and IsValid(r) and r.GetPos then
            local pos, ang = r:GetPos(), r:GetAngles()
            return string.format("%.3f %.3f %.3f %.3f %.3f %.3f", pos.x, pos.y, pos.z, ang.p, ang.y, ang.r)
        end
    end)
    events.on("player.carry_angles", function(p)
        local ent = Entity(p.entity)
        local a = hook.Run("GetPreferredCarryAngles", ent)
        if a == nil and scripted_ents and scripted_ents.Get then
            local t = scripted_ents.Get(ent:GetClass())
            if t and t.GetPreferredCarryAngles then a = t.GetPreferredCarryAngles(ent) end
        end
        if a ~= nil and a.p ~= nil then return string.format("%.3f %.3f %.3f", a.p, a.y, a.r) end
    end)
    events.on("undo.create_request", function(p)
        local tbl = { Owner = Player(p.player), Name = p.name, NiceName = p.text, Entities = {}, Functions = {} }
        if hook.Run("CanCreateUndo", Player(p.player), tbl) == false then return false end
    end)
    events.on("player.respawn_request", function(p) if hook.Run("PlayerDeathThink", Player(p.player)) == true then return false end end)
    events.on("player.suicide_request", function(p) if hook.Run("CanPlayerSuicide", Player(p.player)) == false then return false end end)
    events.on("player.switch_weapon_request", function(p)
        local ply = Player(p.player)
        local W = __glua_classes.Weapon
        if hook.Run("PlayerSwitchWeapon", ply, W.__from(p.from, p.player), W.__from(p.to, p.player)) == true then return false end
    end)
    events.on("sandbox.spawn_request", function(p)
        local ply = Player(p.player)
        local r
        if p.kind == "npc" then r = hook.Run("PlayerSpawnNPC", ply, p.name, "")
        elseif p.kind == "vehicle" then r = hook.Run("PlayerSpawnVehicle", ply, "", p.name, list.Get("Vehicles")[p.name] or {})
        elseif p.kind == "weapon" then r = hook.Run("PlayerSpawnSWEP", ply, p.name, weapons.GetStored(p.name) or {}) end
        if r == false then return false end
    end)
    events.on("sandbox.spawned", function(p)
        local ply, e = Player(p.player), Entity(p.entity)
        if p.kind == "npc" then hook.Run("PlayerSpawnedNPC", ply, e)
        elseif p.kind == "vehicle" then hook.Run("PlayerSpawnedVehicle", ply, e)
        elseif p.kind == "weapon" then hook.Run("PlayerSpawnedSWEP", ply, e) end
    end)
    events.on("player.loadout", function(p) if hook.Run("PlayerLoadout", Player(p.player)) == true then return false end end)
    events.on("prop.spawn_request", function(p) if hook.Run("PlayerSpawnProp", Player(p.player), p.model) == false then return false end end)
    events.on("tool.before", function(p)
        local ply = Player(p.player)
        if hook.Run("CanTool", ply, ply:GetEyeTrace(), p.tool, nil, p.right == 1 and 2 or 1) == false then return false end
    end)
    events.on("player.chat", function(p)
        local r = hook.Run("PlayerSay", Player(p.player), p.text, false)
        if r == "" then return false end
        if type(r) == "string" then return r end
    end)
end


-- Sandbox gamemode defaults (pure Lua in the original gamemode as well): `hook.Run` falls through
-- to these when no addon hook answers.
GM = GM or {}
GAMEMODE = GAMEMODE or GM
function GM:PlayerCanSeePlayersChat(text, teamOnly, listener, speaker) if teamOnly and IsValid(listener) and IsValid(speaker) and listener:Team() ~= speaker:Team() then return false end return true end
function GM:PlayerCanHearPlayersVoice(listener, talker) return true, false end
function GM:PlayerShouldTakeDamage(ply, attacker) return true end
function GM:CanPlayerSuicide(ply) return true end
function GM:IsSpawnpointSuitable(ply, spawn, makeSuitable) return true end
function GM:PlayerCanJoinTeam(ply, team) return true end
function GM:PlayerRequestTeam(ply, team) if hook.Run("PlayerCanJoinTeam", ply, team) == false then return end hook.Run("PlayerJoinTeam", ply, team) end
function GM:PlayerJoinTeam(ply, team) local old = ply:Team(); ply:SetTeam(team); hook.Run("OnPlayerChangedTeam", ply, old, team); hook.Run("PlayerChangedTeam", ply, old, team) end
function GM:OnPlayerChangedTeam(ply, old, new) end
function GM:PlayerChangedTeam(ply, old, new) end
function GM:GetGameDescription() return GM.Name or "Sandbox" end
function GM:CreateTeams() if team and team.SetUp then team.SetUp(TEAM_UNASSIGNED or 1001, "Unassigned", Color(255, 255, 255)); team.SetUp(TEAM_SPECTATOR or 1002, "Spectator", Color(200, 200, 200)) end end
function GM:PlayerDeathThink(ply) end -- the server owns the respawn timer; `true` here keeps the player dead
function GM:PlayerSilentDeath(ply) end
function GM:PlayerDeathSound(ply) return false end
function GM:PlayerFootstep(ply, pos, foot, sound, volume, filter) return false end
function GM:PlayerStepSoundTime(ply, type, walking) return walking and 400 or 300 end
function GM:PlayerShouldTaunt(ply, act) return true end
function GM:PlayerStartTaunt(ply, act, length) end
function GM:PlayerSwitchFlashlight(ply, on) return ply.CanUseFlashlight == nil or true end
function GM:PlayerSpray(ply) return false end
function GM:PlayerCheckLimit(ply, name, current, max) if current >= max then ply:LimitHit(name); return false end return true end
function GM:PlayerCanPickupItem(ply, item) return true end
function GM:PlayerSwitchWeapon(ply, old, new) return false end
function GM:PlayerDroppedWeapon(ply, wep) end
function GM:CanPlayerEnterVehicle(ply, veh, role) return true end
function GM:CanExitVehicle(veh, ply) return true end
function GM:CanCreateUndo(ply, undo) return true end
function GM:OnCleanup(name) end
function GM:PreCleanupMap() end
function GM:PostCleanupMap() end
function GM:CheckPassword(steamID64, ip, svPassword, clPassword, name) return svPassword == "" or svPassword == clPassword end
function GM:OnLuaError(err, realm, stack, name, id) end
function GM:OnNPCDropItem(npc, item) end
function GM:OnDamagedByExplosion(ply, dmginfo) end
function GM:OnCrazyPhysics(ent, phys) end
function GM:GetPreferredCarryAngles(ent) return nil end
function GM:EntityKeyValue(ent, key, value) return nil end
function GM:EntityNetworkedVarChanged(ent, name, old, new) end
function GM:AcceptInput(ent, input, activator, caller, value) return false end
function GM:PlayerSelectSpawn(ply, transition) return NULL end
function GM:PlayerSelectTeamSpawn(team, ply) return NULL end
function GM:PlayerSpawnAsSpectator(ply) ply:Spectate(OBS_MODE_ROAMING or 5) end
function GM:PlayerSetHandsModel(ply, ent) end
function GM:SetPlayerSpeed(ply, walk, run) ply:SetWalkSpeed(walk); ply:SetRunSpeed(run) end
function GM:OnEntityWaterLevelChanged(ent, old, new) end
function GM:PlayerAmmoChanged(ply, id, old, new) end
function GM:PlayerClassChanged(ply, class) end
function GM:OnAchievementAchieved(ply, id) end
function GM:PlayerStartVoice(ply) end
function GM:PlayerEndVoice(ply) end
function GM:OnPlayerHitGround(ply, water, floater, speed) return false end
function GM:OnPlayerJump(ply, speed) end
function GM:EntityFireBullets(ent, data) return true end
function GM:PostEntityFireBullets(ent, data) return true end
function GM:EntityEmitSound(data) return true end
function GM:ShouldCollide(a, b) return true end
function GM:SetupPlayerVisibility(ply, viewEnt) end
function GM:PlayerTraceAttack(ply, dmginfo, dir, tr) return false end
function GM:CanEditVariable(ent, ply, key, val, editor) return ply:IsAdmin() end
function GM:VariableEdited(ent, ply, key, val, editor) if hook.Run("CanEditVariable", ent, ply, key, val, editor) then ent:SetNetworkKeyValue(key, val) end end
function GM:Saved() end
function GM:Restored() end
function GM:PreRegisterSENT(ent, class) return true end
function GM:PreRegisterSWEP(swep, class) return true end


-- navmesh / CNavArea over the own Recast mesh. A "nav area" is a point snapped onto the mesh
-- with the index of the nearest mesh vertex as its ID (the mesh has polygons, not the game's
-- rectangular areas: sizes, corners, adjacency, attributes and ladders are stubs).
local NAVAREA = {}
NAVAREA.__index = NAVAREA
NAVAREA.__eq = function(a, b) return a.__id == b.__id end
NAVAREA.__tostring = function(a) return "CNavArea [" .. a.__id .. "]" end
local nav_verts
local function nvec(t) return Vector(t.x, t.y, t.z) end
local function nav_vertices()
    if nav_verts then return nav_verts end
    local raw = nav.vertices()
    if #raw == 0 then return raw end
    nav_verts = {}
    for i, v in ipairs(raw) do nav_verts[i] = nvec(v) end
    return nav_verts
end
local function area_of(center, id)
    if not id then
        local best = math.huge
        id = 0
        for i, v in ipairs(nav_vertices()) do
            local d = v:DistToSqr(center)
            if d < best then best, id = d, i end
        end
    end
    return setmetatable({ __center = center, __id = id }, NAVAREA)
end
function NAVAREA:GetCenter() return self.__center end
function NAVAREA:GetID() return self.__id end
function NAVAREA:IsValid() return self.__id ~= 0 end
function NAVAREA:GetClosestPointOnArea(pos)
    local r = nav.nearest(pos, { x = 64, y = 64, z = 128 })
    return r and nvec(r) or self.__center
end
function NAVAREA:GetZ(pos)
    local r = nav.nearest({ x = pos.x, y = pos.y, z = self.__center.z }, { x = 1, y = 1, z = 1e5 })
    return r and r.z or self.__center.z
end
function NAVAREA:Contains(pos)
    local r = nav.nearest(pos, { x = 1, y = 1, z = 72 })
    return r ~= nil
end
function NAVAREA:IsVisible(pos)
    return not util.TraceLine({ start = self.__center + Vector(0, 0, 36), endpos = pos }).Hit
end
function NAVAREA:ComputeGroundHeightChange(other) return other.__center.z - self.__center.z end
for _, name in ipairs({ "AddAttributes", "AddHidingSpot", "AddToClosedList", "AddToOpenList", "ClearSearchLists", "ComputeAdjacentConnectionHeightChange", "ComputeDirection", "ConnectTo", "Disconnect", "Draw", "DrawSpots", "GetAdjacentAreaDistances", "GetAdjacentAreas", "GetAdjacentAreasAtSide", "GetAdjacentCount", "GetAdjacentCountAtSide", "GetAttributes", "GetCorner", "GetCostSoFar", "GetExposedSpots", "GetExtentInfo", "GetHidingSpots", "GetIncomingConnectionDistances", "GetIncomingConnections", "GetIncomingConnectionsAtSide", "GetLadders", "GetLaddersAtSide", "GetLightIntensity", "GetParent", "GetParentHow", "GetPlace", "GetRandomAdjacentAreaAtSide", "GetRandomPoint", "GetSizeX", "GetSizeY", "GetSpotEncounters", "GetTotalCost", "GetVisibleAreas", "HasAttributes", "IsBlocked", "IsClosed", "IsCompletelyVisible", "IsConnected", "IsConnectedAtSide", "IsCoplanar", "IsDamaging", "IsFlat", "IsOpen", "IsOpenListEmpty", "IsOverlapping", "IsOverlappingArea", "IsPartiallyVisible", "IsPotentiallyVisible", "IsRoughlySquare", "IsUnderwater", "MarkAsBlocked", "MarkAsDamaging", "MarkAsUnblocked", "PlaceOnGround", "PopOpenList", "Remove", "RemoveAttributes", "RemoveFromClosedList", "SetAttributes", "SetCorner", "SetCostSoFar", "SetParent", "SetPlace", "SetTotalCost", "UpdateOnOpenList" }) do
    if NAVAREA[name] == nil then NAVAREA[name] = stub("CNavArea:" .. name) end
end
__glua_classes.CNavArea = NAVAREA

navmesh = navmesh or {}
function navmesh.IsLoaded() return nav.loaded() end
function navmesh.GetNavAreaCount() return nav.poly_count() end
function navmesh.GetNearestNavArea(pos, anyZ, maxDist, checkLOS, checkGround, team)
    local d = maxDist or 10000
    local r = nav.nearest(pos, { x = d, y = d, z = anyZ and 1e5 or d })
    if not r then return NULL end
    return area_of(nvec(r))
end
function navmesh.GetGroundHeight(pos)
    local r = nav.nearest(pos, { x = 1, y = 1, z = 1e5 })
    if not r then return false end
    return r.z, Vector(0, 0, 1)
end
function navmesh.Find(pos, radius, stepHeight, dropHeight)
    local out, r2 = {}, radius * radius
    for i, v in ipairs(nav_vertices()) do
        local dz = v.z - pos.z
        if (v.x - pos.x) ^ 2 + (v.y - pos.y) ^ 2 <= r2 and dz <= (stepHeight or 18) and -dz <= (dropHeight or 1e9) then
            out[#out + 1] = area_of(v, i)
        end
    end
    return out
end
function navmesh.FindInBox(mins, maxs)
    local out = {}
    for i, v in ipairs(nav_vertices()) do
        if v:WithinAABox(mins, maxs) then out[#out + 1] = area_of(v, i) end
    end
    return out
end
function navmesh.GetAllNavAreas()
    local out = {}
    for i, v in ipairs(nav_vertices()) do out[i] = area_of(v, i) end
    return out
end
function navmesh.GetNavAreaByID(id)
    local v = nav_vertices()[id]
    if not v then return NULL end
    return area_of(v, id)
end
for _, name in ipairs({ "AddWalkableSeed", "BeginGeneration", "ClearWalkableSeeds", "CreateNavArea", "CreateNavLadder", "GetBlockedAreas", "GetEditCursorPosition", "GetMarkedArea", "GetMarkedLadder", "GetNavArea", "GetNavLadderByID", "GetPlayerSpawnName", "IsGenerating", "Load", "Reset", "Save", "SetMarkedArea", "SetMarkedLadder", "SetPlayerSpawnName" }) do
    if navmesh[name] == nil then navmesh[name] = stub("navmesh." .. name) end
end
NAV_MESH_INVALID, NAV_MESH_CROUCH, NAV_MESH_JUMP, NAV_MESH_PRECISE, NAV_MESH_NO_JUMP, NAV_MESH_STOP, NAV_MESH_RUN, NAV_MESH_WALK, NAV_MESH_AVOID, NAV_MESH_TRANSIENT, NAV_MESH_DONT_HIDE, NAV_MESH_STAND, NAV_MESH_NO_HOSTAGES, NAV_MESH_STAIRS, NAV_MESH_NO_MERGE, NAV_MESH_OBSTACLE_TOP, NAV_MESH_CLIFF = 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768
NAV_MESH_FUNC_COST, NAV_MESH_HAS_ELEVATOR, NAV_MESH_NAV_BLOCKER = 536870912, 1073741824, -2147483648
NORTH, EAST, SOUTH, WEST = 0, 1, 2, 3
