-- GLua entity surface over the host snapshot (`world.get / find / players / count`) and the
-- write queue (`world.queue`): Entity / NPC / Vehicle / Weapon / PhysObj / Player classes, the
-- ents / player / game / engine / util libraries and the entity globals. Reads see the tick's
-- snapshot, writes land in the serial phase (an entity created with ents.Create binds to its
-- id when `entity.spawned` reports the spawn tag).
local queue = world and world.queue
local function qwrite(op, ent, arg)
    if not queue then error(op .. ": no world access") end
    return queue(op, ent, arg)
end
local function snap(id) return world and world.get(id) or nil end

------------------------------------------------------------------------ Entity
local ENT = {}
ENT.__index = function(self, k)
    local m = rawget(ENT, k)
    if m ~= nil then return m end
    local sub = rawget(self, "__class_meta")
    if sub then local f = sub[k]; if f ~= nil then return f end end
    return rawget(self, "__vars")[k]
end
ENT.__newindex = function(self, k, v) rawget(self, "__vars")[k] = v end
ENT.__tostring = function(self)
    if not self:IsValid() then return "[NULL Entity]" end
    return "Entity [" .. self:EntIndex() .. "][" .. self:GetClass() .. "]"
end
ENT.__eq = function(a, b) return rawget(a, "__id") == rawget(b, "__id") and rawget(a, "__id") ~= nil end
local entities = setmetatable({}, { __mode = "v" })
local function wrap(id, class_meta)
    local e = entities[id]
    if e then return e end
    e = setmetatable({ __id = id, __vars = {}, __class_meta = class_meta, __tables = {} }, ENT)
    entities[id] = e
    return e
end
NULL = setmetatable({ __id = 0, __vars = {}, __tables = {} }, ENT)
local NPC, VEH, WEP = {}, {}, {}
local function class_meta(class)
    if class:sub(1, 4) == "npc_" then return NPC end
    if class:sub(1, 7) == "weapon_" then return WEP end
    if class == "prop_vehicle_jeep" or class == "prop_vehicle_airboat" or class == "prop_vehicle_prisoner_pod" or class:sub(1, 13) == "prop_vehicle_" then return VEH end
    return nil
end
function Entity(id)
    if type(id) ~= "number" or id <= 0 then return NULL end
    local s = snap(id)
    local e = wrap(id, s and class_meta(s.class) or nil)
    if s and rawget(e, "__class_meta") == nil then rawset(e, "__class_meta", class_meta(s.class)) end
    return e
end
local function isent(v) return type(v) == "table" and getmetatable(v) == ENT end
function ENT:EntIndex() return rawget(self, "__id") or -1 end
function ENT:IsValid()
    local id = rawget(self, "__id")
    if id == nil or id <= 0 then return false end
    return snap(id) ~= nil
end
function ENT:IsPlayer() return false end
function ENT:IsWorld() return false end
function ENT:IsNPC() local s = snap(self.__id); return s ~= nil and s.class:sub(1, 4) == "npc_" end
function ENT:IsNextBot() return false end
function ENT:IsVehicle() local s = snap(self.__id); return s ~= nil and s.class:sub(1, 13) == "prop_vehicle_" end
function ENT:IsWeapon() local s = snap(self.__id); return s ~= nil and s.class:sub(1, 7) == "weapon_" end
function ENT:IsRagdoll() local s = snap(self.__id); return s ~= nil and s.class == "prop_ragdoll" end
function ENT:IsScripted() return false end
function ENT:GetClass() local s = snap(self.__id); return s and s.class or "" end
function ENT:GetModel() local s = snap(self.__id); return s and s.model or nil end
function ENT:GetPos() local s = snap(self.__id); return s and Vector(s.pos.x, s.pos.y, s.pos.z) or Vector() end
function ENT:GetAngles() local s = snap(self.__id); return s and Angle(s.angles.x, s.angles.y, s.angles.z) or Angle() end
function ENT:GetVelocity() local s = snap(self.__id); return s and Vector(s.velocity.x, s.velocity.y, s.velocity.z) or Vector() end
function ENT:GetForward() return self:GetAngles():Forward() end
function ENT:GetRight() return self:GetAngles():Right() end
function ENT:GetUp() return self:GetAngles():Up() end
function ENT:EyePos() return self:GetPos() end
function ENT:EyeAngles() return self:GetAngles() end
function ENT:WorldSpaceCenter() return self:GetPos() end
function ENT:LocalToWorld(v)
    local r = Vector(v.x, v.y, v.z)
    r:Rotate(self:GetAngles())
    return r + self:GetPos()
end
function ENT:WorldToLocal(v)
    local d = v - self:GetPos()
    local a = self:GetAngles()
    return Vector(d:Dot(a:Forward()), -d:Dot(a:Right()), d:Dot(a:Up()))
end
function ENT:LocalToWorldAngles(a) return self:GetAngles() + a end
function ENT:WorldToLocalAngles(a) return a - self:GetAngles() end
function ENT:GetOwner() local s = snap(self.__id); if s and s.owner ~= 0 then return Player(s.owner) end return NULL end
function ENT:GetCreator() return self:GetOwner() end
function ENT:CPPIGetOwner() return self:GetOwner() end
function ENT:GetSkin() local s = snap(self.__id); return s and s.skin or 0 end
function ENT:Health() local s = snap(self.__id); return s and s.health or 0 end
function ENT:GetMaxHealth() local s = snap(self.__id); return s and s.max_health or 0 end
function ENT:IsPlayerHolding() return false end
function ENT:GetBodygroup(part)
    local s = snap(self.__id)
    if not s or s.body == "" then return 0 end
    local c = s.body:sub(part + 1, part + 1)
    return c == "" and 0 or (tonumber(c) or (c:byte() - 97))
end
-- writes (serial phase)
local function v3(v) return { x = v.x, y = v.y, z = v.z } end
local function a3(a) return { x = a.p, y = a.y, z = a.r } end
function ENT:SetPos(v)
    local pend = rawget(self, "__pending")
    if pend then pend.pos = v3(v) return end
    qwrite("set_pos", self.__id, v3(v))
end
function ENT:SetAngles(a)
    local pend = rawget(self, "__pending")
    if pend then pend.angles = a3(a) return end
    qwrite("set_angles", self.__id, a3(a))
end
function ENT:SetModel(m)
    local pend = rawget(self, "__pending")
    if pend then pend.model = m return end
    error("Entity:SetModel: the model of a spawned entity is fixed here")
end
function ENT:SetVelocity(v) qwrite("set_velocity", self.__id, v3(v)) end
function ENT:SetLocalVelocity(v) qwrite("set_velocity", self.__id, v3(v)) end
function ENT:SetHealth(h) qwrite("set_health", self.__id, h) end
function ENT:SetMaxHealth(h) rawget(self, "__vars").__max_health = h end
function ENT:Remove() qwrite("remove", self.__id, nil) end
function ENT:SetSkin(n) qwrite("set_skin", self.__id, n) end
function ENT:SetBodygroup(part, value) qwrite("set_body", self.__id, { part = part, model = value }) end
function ENT:Ignite(seconds) qwrite("ignite", self.__id, seconds or 30) end
function ENT:Extinguish() qwrite("extinguish", self.__id, nil) end
function ENT:SetCollisionGroup(group) qwrite("set_collide", self.__id, group ~= COLLISION_GROUP_WORLD and group ~= COLLISION_GROUP_IN_VEHICLE) end
function ENT:SetSolid(solid) qwrite("set_collide", self.__id, solid ~= SOLID_NONE) end
function ENT:NextThink(t) qwrite("next_think", self.__id, math.max(0, t - CurTime())) return true end
function ENT:SetOwner(ply) rawget(self, "__vars").__owner = ply end
function ENT:SetName(n) rawget(self, "__vars").__name = n end
function ENT:GetName() return rawget(self, "__vars").__name or "" end
function ENT:SetNWString(k, v) self:SetNetworkedVar(k, v) end
function ENT:SetNWInt(k, v) self:SetNetworkedVar(k, v) end
function ENT:SetNWFloat(k, v) self:SetNetworkedVar(k, v) end
function ENT:SetNWBool(k, v) self:SetNetworkedVar(k, v) end
function ENT:SetNWVector(k, v) self:SetNetworkedVar(k, v) end
function ENT:SetNWAngle(k, v) self:SetNetworkedVar(k, v) end
function ENT:SetNWEntity(k, v) self:SetNetworkedVar(k, v) end
function ENT:SetNetworkedVar(k, v) rawget(self, "__vars").__nw = rawget(self, "__vars").__nw or {}; rawget(self, "__vars").__nw[k] = v end
function ENT:GetNetworkedVar(k, default) local nw = rawget(self, "__vars").__nw; if nw and nw[k] ~= nil then return nw[k] end return default end
function ENT:GetNWString(k, d) return self:GetNetworkedVar(k, d or "") end
function ENT:GetNWInt(k, d) return self:GetNetworkedVar(k, d or 0) end
function ENT:GetNWFloat(k, d) return self:GetNetworkedVar(k, d or 0) end
function ENT:GetNWBool(k, d) return self:GetNetworkedVar(k, d or false) end
function ENT:GetNWVector(k, d) return self:GetNetworkedVar(k, d or Vector()) end
function ENT:GetNWAngle(k, d) return self:GetNetworkedVar(k, d or Angle()) end
function ENT:GetNWEntity(k, d) return self:GetNetworkedVar(k, d or NULL) end
ENT.SetNW2String, ENT.SetNW2Int, ENT.SetNW2Float, ENT.SetNW2Bool, ENT.SetNW2Vector, ENT.SetNW2Angle, ENT.SetNW2Entity = ENT.SetNWString, ENT.SetNWInt, ENT.SetNWFloat, ENT.SetNWBool, ENT.SetNWVector, ENT.SetNWAngle, ENT.SetNWEntity
ENT.GetNW2String, ENT.GetNW2Int, ENT.GetNW2Float, ENT.GetNW2Bool, ENT.GetNW2Vector, ENT.GetNW2Angle, ENT.GetNW2Entity = ENT.GetNWString, ENT.GetNWInt, ENT.GetNWFloat, ENT.GetNWBool, ENT.GetNWVector, ENT.GetNWAngle, ENT.GetNWEntity
function ENT:GetTable() return rawget(self, "__vars") end
function ENT:SetVar(k, v) rawget(self, "__vars")[k] = v end
function ENT:GetVar(k, d) local v = rawget(self, "__vars")[k]; if v == nil then return d end return v end
function ENT:SetSaveValue(k, v) rawget(self, "__vars")[k] = v return true end
function ENT:GetSaveTable() return table.Copy(rawget(self, "__vars")) end
function ENT:GetInternalVariable(k) return rawget(self, "__vars")[k] end
function ENT:IsValidLayer() return false end
function ENT:GetCollisionBounds() return Vector(), Vector() end
function ENT:OBBCenter() return Vector() end
function ENT:Spawn()
    local pend = rawget(self, "__pending")
    if not pend then return end
    rawset(self, "__pending", nil)
    qwrite("spawn", 0, { class = pend.class ~= "prop_physics" and pend.class or nil, model = pend.model or "", pos = pend.pos or { x = 0, y = 0, z = 0 }, angles = pend.angles, owner = pend.owner or 0, tag = pend.tag })
end
function ENT:Activate() end
function ENT:GetPhysicsObject()
    local P = {}
    P.__index = P
    local ent = self
    function P:IsValid() return ent:IsValid() end
    function P:GetPos() return ent:GetPos() end
    function P:GetAngles() return ent:GetAngles() end
    function P:GetVelocity() return ent:GetVelocity() end
    function P:SetVelocity(v) ent:SetVelocity(v) end
    function P:SetVelocityInstantaneous(v) ent:SetVelocity(v) end
    function P:AddVelocity(v) ent:SetVelocity(ent:GetVelocity() + v) end
    function P:SetPos(v) ent:SetPos(v) end
    function P:SetAngles(a) ent:SetAngles(a) end
    function P:EnableMotion(on) qwrite("freeze", ent.__id, not on) end
    function P:IsMotionEnabled() local s = snap(ent.__id); return s ~= nil and not s.frozen end
    function P:IsMoveable() return self:IsMotionEnabled() end
    function P:IsAsleep() return false end
    function P:Wake() end
    function P:Sleep() end
    function P:EnableGravity(on) qwrite("set_gravity", ent.__id, on) end
    function P:EnableCollisions(on) qwrite("set_collide", ent.__id, on) end
    function P:EnableDrag(on) end
    function P:GetEntity() return ent end
    function P:GetMass() return 1 end
    function P:SetMass() end
    function P:GetMassCenter() return Vector() end
    function P:ApplyForceCenter(f) ent:SetVelocity(ent:GetVelocity() + f / math.max(self:GetMass(), 1e-3) * FrameTime()) end
    function P:GetMaterial() return "" end
    function P:SetMaterial() end
    function P:LocalToWorld(v) return ent:LocalToWorld(v) end
    function P:WorldToLocal(v) return ent:WorldToLocal(v) end
    return setmetatable({}, P)
end
__glua_classes.PhysObj = ENT:GetPhysicsObject() and getmetatable(NULL:GetPhysicsObject()) or {}

------------------------------------------------------------------------ NPC / Vehicle / Weapon
function NPC:SetEnemy(ply) if ply and ply.IsPlayer and ply:IsPlayer() then qwrite("npc_target", self.__id, { player = ply:UserID() }) else qwrite("npc_target", self.__id, {}) end end
function NPC:SetTarget(ply) self:SetEnemy(ply) end
function NPC:NavSetGoalPos(pos) return qwrite("npc_goto", self.__id, { x = pos.x, y = pos.y, z = pos.z, run = rawget(self, "__vars").__run or false }) end
function NPC:SetLastPosition(pos) self:NavSetGoalPos(pos) end
function NPC:NavSetGoalTarget(ent) if ent.IsPlayer and ent:IsPlayer() then qwrite("npc_chase", self.__id, { player = ent:UserID(), run = rawget(self, "__vars").__run or false }) end end
function NPC:ClearGoal() qwrite("npc_stop", self.__id, nil) end
function NPC:StopMoving() qwrite("npc_stop", self.__id, nil) end
function NPC:SetMovementActivity(act) rawget(self, "__vars").__run = act == ACT_RUN end
function NPC:GetActiveWeapon() local s = snap(self.__id); return s and WEP.__from(s.class) or NULL end
function NPC:GetNPCState() return 0 end
function NPC:IsMoving() local s = snap(self.__id); return s ~= nil and (s.velocity.x ~= 0 or s.velocity.y ~= 0) end
function VEH:GetDriver() return NULL end
function VEH:GetVehicleClass() return self:GetClass() end
function VEH:IsEngineEnabled() return true end
function VEH:GetThrottle() return 0 end
function VEH:GetSteering() return 0 end
function WEP:GetPrintName() return self:GetClass() end
function WEP:Clip1() return -1 end
function WEP:Clip2() return -1 end
function WEP:GetMaxClip1() return -1 end
function WEP:GetMaxClip2() return -1 end
function WEP:GetHoldType() return "normal" end
function WEP:GetSlot() return 0 end
function WEP:GetSlotPos() return 0 end
function WEP:IsCarriedByLocalPlayer() return false end
function WEP:IsScripted() return false end
function WEP:IsWeapon() return true end
-- a carried weapon has no entity id of its own here: a light wrapper by class
local carried = {}
function WEP.__from(class, owner)
    local w = carried[class .. ":" .. tostring(owner)]
    if w then return w end
    w = setmetatable({ __id = 0, __vars = {}, __class_meta = WEP, __tables = {}, __class = class, __owner = owner }, ENT)
    carried[class .. ":" .. tostring(owner)] = w
    return w
end
local base_class = ENT.GetClass
function ENT:GetClass() local c = rawget(self, "__class"); if c then return c end return base_class(self) end
local base_valid = ENT.IsValid
function ENT:IsValid() if rawget(self, "__class") then local o = rawget(self, "__owner"); return o ~= nil and Player(o):HasWeapon(rawget(self, "__class")) end return base_valid(self) end
function WEP:GetOwner() local o = rawget(self, "__owner"); return o and Player(o) or NULL end

------------------------------------------------------------------------ Player
local PLY = {}
PLY.__index = function(self, k)
    local m = rawget(PLY, k)
    if m ~= nil then return m end
    return rawget(self, "__vars")[k]
end
PLY.__newindex = ENT.__newindex
PLY.__eq = function(a, b) return rawget(a, "__pid") == rawget(b, "__pid") end
PLY.__tostring = function(self)
    if not self:IsValid() then return "[NULL Player]" end
    return "Player [" .. self:UserID() .. "][" .. self:Nick() .. "]"
end
local players = {}
local function psnap(id)
    if not world then return nil end
    for _, p in ipairs(world.players()) do if p.id == id then return p end end
    return nil
end
function Player(id)
    if type(id) ~= "number" or id <= 0 then return NULL end
    local p = players[id]
    if p then return p end
    p = setmetatable({ __pid = id, __vars = {}, __tables = {} }, PLY)
    players[id] = p
    return p
end
-- shared Entity methods a Player answers the same way
for _, k in ipairs({ "GetForward", "GetRight", "GetUp", "SetNWString", "SetNWInt", "SetNWFloat", "SetNWBool", "SetNWVector", "SetNWAngle", "SetNWEntity", "SetNetworkedVar", "GetNetworkedVar", "GetNWString", "GetNWInt", "GetNWFloat", "GetNWBool", "GetNWVector", "GetNWAngle", "GetNWEntity", "GetTable", "SetVar", "GetVar", "GetName", "SetName", "LocalToWorld", "WorldToLocal", "IsNPC", "IsVehicle", "IsWeapon", "IsWorld", "IsRagdoll", "IsNextBot", "IsScripted", "SetNW2String", "SetNW2Int", "SetNW2Float", "SetNW2Bool", "SetNW2Vector", "SetNW2Angle", "SetNW2Entity", "GetNW2String", "GetNW2Int", "GetNW2Float", "GetNW2Bool", "GetNW2Vector", "GetNW2Angle", "GetNW2Entity" }) do
    PLY[k] = ENT[k]
end
function PLY:IsPlayer() return true end
function PLY:IsNPC() return false end
function PLY:IsValid() return psnap(self.__pid) ~= nil end
function PLY:EntIndex() return self.__pid end
function PLY:UserID() return self.__pid end
function PLY:AccountID() return self.__pid end
function PLY:IsBot() local p = psnap(self.__pid); return p ~= nil and p.identity == nil end
function PLY:IsAdmin() return rawget(self, "__vars").__group == "admin" or rawget(self, "__vars").__group == "superadmin" end
function PLY:IsSuperAdmin() return rawget(self, "__vars").__group == "superadmin" end
function PLY:SetUserGroup(g) rawget(self, "__vars").__group = g end
function PLY:GetUserGroup() return rawget(self, "__vars").__group or "user" end
function PLY:IsUserGroup(g) return self:GetUserGroup() == g end
function PLY:Nick() local p = psnap(self.__pid); return p and (p.identity or ("Player " .. self.__pid)) or "" end
PLY.Name = PLY.Nick
PLY.GetName = PLY.Nick
function PLY:SteamID() local p = psnap(self.__pid); return p and p.identity or "" end
PLY.SteamID64 = PLY.SteamID
function PLY:GetClass() return "player" end
function PLY:GetModel() local p = psnap(self.__pid); return p and p.model or "" end
function PLY:GetSkin() local p = psnap(self.__pid); return p and p.skin or 0 end
function PLY:GetPos() local p = psnap(self.__pid); return p and Vector(p.origin.x, p.origin.y, p.origin.z) or Vector() end
function PLY:GetVelocity() local p = psnap(self.__pid); return p and Vector(p.velocity.x, p.velocity.y, p.velocity.z) or Vector() end
function PLY:EyeAngles() local p = psnap(self.__pid); return p and Angle(p.view_angles.x, p.view_angles.y, p.view_angles.z) or Angle() end
function PLY:GetAngles() local a = self:EyeAngles(); return Angle(0, a.y, 0) end
function PLY:GetAimVector() return self:EyeAngles():Forward() end
function PLY:EyePos() return self:GetPos() + Vector(0, 0, 64) end
function PLY:GetShootPos() return self:EyePos() end
function PLY:WorldSpaceCenter() return self:GetPos() + Vector(0, 0, 36) end
function PLY:Alive() local p = psnap(self.__pid); return p ~= nil and p.alive end
function PLY:Health() local p = psnap(self.__pid); return p and p.health or 0 end
function PLY:Armor() local p = psnap(self.__pid); return p and p.armor or 0 end
function PLY:GetMaxHealth() local p = psnap(self.__pid); return p and p.max_health or 100 end
function PLY:GetMaxArmor() return 100 end
function PLY:Frags() local p = psnap(self.__pid); return p and p.frags or 0 end
function PLY:Deaths() local p = psnap(self.__pid); return p and p.deaths or 0 end
function PLY:GetWeapons()
    local p = psnap(self.__pid)
    local out = {}
    if p then for _, c in ipairs(p.weapons) do out[#out + 1] = WEP.__from(c, self.__pid) end end
    return out
end
function PLY:HasWeapon(class) local p = psnap(self.__pid); if not p then return false end for _, c in ipairs(p.weapons) do if c == class then return true end end return false end
function PLY:GetWeapon(class) if self:HasWeapon(class) then return WEP.__from(class, self.__pid) end return NULL end
function PLY:GetActiveWeapon() local p = psnap(self.__pid); if p and p.active_weapon ~= "" then return WEP.__from(p.active_weapon, self.__pid) end return NULL end
function PLY:KeyDown() return false end
function PLY:InVehicle() return false end
function PLY:GetVehicle() return NULL end
function PLY:IsFrozen() return false end
function PLY:IsOnGround() return true end
function PLY:Crouching() return false end
function PLY:IsSprinting() return false end
function PLY:IsTyping() return false end
function PLY:IsSpeaking() return false end
function PLY:Team() return rawget(self, "__vars").__team or 1 end
function PLY:SetTeam(t) rawget(self, "__vars").__team = t end
function PLY:GetEyeTrace()
    return util.TraceLine({ start = self:EyePos(), endpos = self:EyePos() + self:GetAimVector() * 56756, filter = self })
end
PLY.GetEyeTraceNoCursor = PLY.GetEyeTrace
function PLY:Ping() return 0 end
function PLY:IsConnected() return self:IsValid() end
function PLY:IsFullyAuthenticated() return true end
function PLY:IsListenServerHost() return false end
-- writes
function PLY:SetPos(v) qwrite("player.teleport", self.__pid, v3(v)) end
function PLY:SetHealth(h) qwrite("player.set_health", self.__pid, h) end
function PLY:SetArmor(a) qwrite("player.set_armor", self.__pid, a) end
function PLY:Kill() qwrite("player.kill", self.__pid, nil) end
function PLY:KillSilent() qwrite("player.kill", self.__pid, nil) end
function PLY:Give(class) qwrite("player.give_weapon", self.__pid, class) return WEP.__from(class, self.__pid) end
function PLY:SelectWeapon(class) qwrite("player.select_weapon", self.__pid, class) end
function PLY:TakeDamage(amount, attacker, inflictor)
    qwrite("player.damage", self.__pid, { amount = amount, attacker = attacker and attacker.UserID and attacker:UserID() or 0, type = DMG_GENERIC })
end
function PLY:TakeDamageInfo(info) qwrite("player.damage", self.__pid, { amount = info:GetDamage(), attacker = info:GetAttacker().UserID and info:GetAttacker():UserID() or 0, type = info:GetDamageType() }) end
function PLY:SetVelocity(v) end
function PLY:PrintMessage(kind, msg) print(msg) end
function PLY:ChatPrint(msg) print(msg) end
function PLY:ConCommand(cmd) end
function PLY:SendLua(code) end
function PLY:Spawn() end
function PLY:StripWeapons() end
function PLY:StripWeapon(class) end
function PLY:Freeze(on) end
function PLY:GodEnable() rawget(self, "__vars").__god = true end
function PLY:GodDisable() rawget(self, "__vars").__god = false end
function PLY:HasGodMode() return rawget(self, "__vars").__god == true end
function PLY:SetModel(m) end
function PLY:SetNoDraw() end
function PLY:SetMaxHealth(h) rawget(self, "__vars").__max_health = h end
function PLY:GetInfo(cvar) return "" end
function PLY:GetInfoNum(cvar, default) return default end
function PLY:SetPData(k, v) rawget(self, "__vars").__pdata = rawget(self, "__vars").__pdata or {}; rawget(self, "__vars").__pdata[k] = v return true end
function PLY:GetPData(k, d) local pd = rawget(self, "__vars").__pdata; if pd and pd[k] ~= nil then return pd[k] end return d end
function PLY:RemovePData(k) local pd = rawget(self, "__vars").__pdata; if pd then pd[k] = nil end return true end
function PLY:GetCount(kind) return 0 end
function PLY:CheckLimit(kind) return true end
function PLY:AddCount() end
function PLY:AddCleanup() end
function PLY:GetTool() return nil end
function PLY:IsWorldClicking() return false end
function PLY:UniqueID() return self.__pid end
function PLY:GetViewEntity() return self end
function PLY:GetObserverMode() return 0 end
function PLY:GetFOV() return 90 end
function PLY:GetDuckSpeed() return 0.3 end
function PLY:GetWalkSpeed() return 200 end
function PLY:GetRunSpeed() return 400 end
function PLY:GetJumpPower() return 200 end
function PLY:GetGravity() return 1 end
function PLY:SetWalkSpeed() end
function PLY:SetRunSpeed() end
function PLY:SetJumpPower() end
function PLY:SetGravity() end
function PLY:SetDuckSpeed() end

------------------------------------------------------------------------ ents / player / game / engine
ents = {}
local function all_ids() return world and world.find(nil) or {} end
function ents.GetAll()
    local out = {}
    for _, e in ipairs(all_ids()) do out[#out + 1] = Entity(e.id) end
    for _, p in ipairs(world and world.players() or {}) do out[#out + 1] = Player(p.id) end
    return out
end
function ents.GetCount(ignoreTemp) return (world and world.count() or 0) + #(world and world.players() or {}) end
function ents.GetEdictCount() return ents.GetCount() end
function ents.GetByIndex(i) return Entity(i) end
function ents.Iterator()
    local list = ents.GetAll()
    return ipairs(list)
end
function ents.FindByClass(pattern)
    local out = {}
    local pat = "^" .. string.gsub(string.PatternSafe(pattern), "%%%*", ".*") .. "$"
    for _, e in ipairs(all_ids()) do if string.find(e.class, pat) then out[#out + 1] = Entity(e.id) end end
    if string.find("player", pat) then for _, p in ipairs(world and world.players() or {}) do out[#out + 1] = Player(p.id) end end
    return out
end
function ents.FindByModel(model)
    local out = {}
    for _, e in ipairs(world and world.find(model) or {}) do out[#out + 1] = Entity(e.id) end
    return out
end
function ents.FindByName(name)
    local out = {}
    for _, e in ipairs(ents.GetAll()) do if e:GetName() == name then out[#out + 1] = e end end
    return out
end
function ents.FindInSphere(origin, radius)
    local out = {}
    local r2 = radius * radius
    for _, e in ipairs(ents.GetAll()) do if e:GetPos():DistToSqr(origin) <= r2 then out[#out + 1] = e end end
    return out
end
function ents.FindInBox(mins, maxs)
    local out = {}
    for _, e in ipairs(ents.GetAll()) do if e:GetPos():WithinAABox(mins, maxs) then out[#out + 1] = e end end
    return out
end
function ents.FindInCone(origin, dir, dist, radius)
    local out = {}
    local d = dir:GetNormalized()
    for _, e in ipairs(ents.GetAll()) do
        local to = e:GetPos() - origin
        local l = to:Length()
        if l <= dist and l > 0 and to:Dot(d) / l >= radius then out[#out + 1] = e end
    end
    return out
end
function ents.FindByClassAndParent(class) return ents.FindByClass(class) end
function ents.FindAlongRay(start, finish)
    local out = {}
    local d = finish - start
    local len = d:Length()
    if len < 1e-6 then return out end
    local dir = d / len
    for _, e in ipairs(ents.GetAll()) do
        local to = e:GetPos() - start
        local t = math.Clamp(to:Dot(dir), 0, len)
        if (start + dir * t):DistToSqr(e:GetPos()) <= 32 * 32 then out[#out + 1] = e end
    end
    return out
end
local spawn_seq = 0
local pending_by_tag = {}
function ents.Create(class)
    spawn_seq += 1
    local tag = "glua:" .. tostring(game and game.tick and game.tick() or 0) .. ":" .. spawn_seq
    local e = setmetatable({ __id = 0, __vars = {}, __class_meta = class_meta(class), __tables = {}, __pending = { class = class, tag = tag } }, ENT)
    pending_by_tag[tag] = e
    return e
end
ents.CreateClientProp = ents.Create
function ents.CreateClientside(class) return ents.Create(class) end
function ents.GetMapCreatedEntity() return NULL end
if events then
    events.on("entity.spawned", function(p)
        local e = p.tag and pending_by_tag[p.tag]
        if e then
            pending_by_tag[p.tag] = nil
            rawset(e, "__id", p.entity)
            entities[p.entity] = e
            hook.Run("PlayerSpawnedProp", Player(p.player), e:GetModel(), e)
        end
    end)
    events.on("player.disconnected", function(p) hook.Run("PlayerDisconnected", Player(p.player)) end)
    events.on("player.spawned", function(p) hook.Run("PlayerSpawn", Player(p.player)) end)
end

player = {}
function player.GetAll()
    local out = {}
    for _, p in ipairs(world and world.players() or {}) do out[#out + 1] = Player(p.id) end
    return out
end
function player.GetCount() return #(world and world.players() or {}) end
function player.GetByID(id) return Player(id) end
function player.GetByUniqueID(id) return Player(tonumber(id) or 0) end
function player.GetBySteamID(sid) for _, p in ipairs(player.GetAll()) do if p:SteamID() == sid then return p end end return false end
player.GetBySteamID64 = player.GetBySteamID
function player.GetByAccountID(id) return Player(id) end
function player.GetHumans() local out = {}; for _, p in ipairs(player.GetAll()) do if not p:IsBot() then out[#out + 1] = p end end return out end
function player.GetBots() local out = {}; for _, p in ipairs(player.GetAll()) do if p:IsBot() then out[#out + 1] = p end end return out end
function player.Iterator() return ipairs(player.GetAll()) end
function player.CreateNextBot() return NULL end

game = game or {}
function game.GetMap() return (cvar and cvar.get and cvar.get("map")) or "" end
function game.MaxPlayers() return tonumber(cvar and cvar.get and cvar.get("maxplayers")) or 128 end
function game.SinglePlayer() return false end
function game.IsDedicated() return true end
function game.GetTimeScale() return 1 end
function game.GetIPAddress() return "" end
function game.GetWorld() return NULL end
function game.GetGlobalState() return 0 end
function game.GetSkillLevel() return tonumber(cvar and cvar.get and cvar.get("skill")) or 1 end
function game.GetAmmoID(name) return 0 end
function game.GetAmmoName(id) return nil end
function game.GetAmmoMax() return 9999 end
function game.GetAmmoTypes() return {} end
function game.GetAmmoData() return nil end
function game.AddAmmoType() end
function game.AddDecal() end
function game.AddParticles() end
function game.MountGMA() return false end
function game.GetMapNext() return "" end
function game.GetMapVersion() return 0 end
function game.StartSpot() return Vector() end
function game.MaxPlayers() return 128 end
function game.SetGlobalState() end
function game.GetSunInfo() return nil end
function game.KickID() end
function game.RemoveRagdolls() end
function game.SetTimeScale() end
function game.GetGlobalCounter() return 0 end
function game.SetGlobalCounter() end
function game.CleanUpMap() end
function game.ConsoleCommand() end
function game.LoadNextMap() end
function game.BuildAmmoTypes() end
function game.SetSkillLevel() end

engine = engine or {}
function engine.TickInterval() return game.interval and game.interval() or 0.015 end
function engine.TickCount() return game.tick and game.tick() or 0 end
function engine.ActiveGamemode() return "sandbox" end
function engine.GetGamemodes() return { { name = "sandbox", title = "Sandbox" } } end
function engine.GetAddons() return {} end
function engine.GetGames() return {} end
function engine.GetUserContent() return {} end
function engine.ServerFrameTime() return engine.TickInterval(), 0 end
function engine.AbsoluteFrameTime() return engine.TickInterval() end
function engine.GetDemoPlaybackStartTick() return 0 end
function engine.GetDemoPlaybackTick() return 0 end
function engine.GetDemoPlaybackTimeScale() return 1 end
function engine.GetDemoPlaybackTotalTicks() return 0 end
function engine.IsPlayingDemo() return false end
function engine.IsRecordingDemo() return false end
function engine.LightStyle() end
function engine.CloseServer() end
function engine.SetMounted() end
function engine.GetMountedContent() return {} end
function engine.GetDemoPlaybackData() return nil end

------------------------------------------------------------------------ util (traces, json, misc)
util = util or {}
local function trace_result(tr, start, endpos, filter)
    local hit = tr and tr.hit or false
    local pos = hit and Vector(tr.pos.x, tr.pos.y, tr.pos.z) or Vector(endpos.x, endpos.y, endpos.z)
    local ent = NULL
    if hit and tr.ent ~= 0 then ent = Entity(tr.ent) end
    local hitworld = hit and tr.ent == 0
    return {
        Hit = hit, HitPos = pos, HitNormal = hit and Vector(tr.normal.x, tr.normal.y, tr.normal.z) or Vector(),
        Entity = ent, Fraction = tr and tr.fraction or 1, FractionLeftSolid = 0, StartPos = Vector(start.x, start.y, start.z),
        HitWorld = hitworld, HitNonWorld = hit and not hitworld, HitSky = false, HitNoDraw = false, HitBox = 0, HitGroup = 0,
        HitTexture = hitworld and "**studio**" or "", MatType = 0, Normal = (endpos - start):GetNormalized(),
        PhysicsBone = 0, StartSolid = false, AllSolid = false, SurfaceProps = 0, SurfaceFlags = 0, DispFlags = 0, Contents = 0,
    }
end
function util.TraceLine(data)
    local start, endpos = data.start, data.endpos
    local tr = phys and phys.trace and phys.trace({ x = start.x, y = start.y, z = start.z }, { x = endpos.x, y = endpos.y, z = endpos.z }) or nil
    local r = trace_result(tr, start, endpos, data.filter)
    if data.output then for k, v in pairs(r) do data.output[k] = v end end
    return r
end
function util.TraceHull(data) return util.TraceLine(data) end
function util.TraceEntity(data, ent) return util.TraceLine(data) end
function util.QuickTrace(origin, dir, filter) return util.TraceLine({ start = origin, endpos = origin + dir, filter = filter }) end
function util.GetSurfacePropName() return "" end
function util.GetSurfaceIndex() return 0 end
function util.GetSurfaceData() return nil end
function util.PointContents(pos)
    local tr = util.TraceLine({ start = pos, endpos = pos })
    return tr.Hit and CONTENTS_SOLID or CONTENTS_EMPTY
end
function util.IsInWorld(pos) return math.abs(pos.x) < 16384 and math.abs(pos.y) < 16384 and math.abs(pos.z) < 16384 end
function util.IsValidModel(m) return type(m) == "string" and m:sub(-4) == ".mdl" end
function util.IsValidProp(m) return util.IsValidModel(m) end
function util.IsValidRagdoll(m) return util.IsValidModel(m) end
function util.IsValidPhysicsObject(ent) return IsValid(ent) end
function util.IsModelLoaded(m) return util.IsValidModel(m) end
function util.PrecacheModel() end
function util.PrecacheSound() end
function util.SteamIDFrom64(s) return "STEAM_0:0:0" end
function util.SteamIDTo64(s) return "0" end
function util.TypeToString(v) return tostring(v) end
function util.StringToType(s, t)
    if t == "number" or t == "float" or t == "int" then return tonumber(s) end
    if t == "bool" or t == "boolean" then return tobool(s) end
    if t == "vector" then return Vector(s) end
    if t == "angle" then return Angle(s) end
    return s
end
function util.DistanceToLine(a, b, p)
    local d = b - a
    local l2 = d:LengthSqr()
    local t = l2 > 0 and math.Clamp((p - a):Dot(d) / l2, 0, 1) or 0
    local nearest = a + d * t
    return nearest:Distance(p), nearest, t
end
function util.IntersectRayWithOBB(start, delta, boxOrigin, boxAngles, mins, maxs)
    local l = start - boxOrigin
    local a = boxAngles
    local ls = Vector(l:Dot(a:Forward()), -l:Dot(a:Right()), l:Dot(a:Up()))
    local ld = Vector(delta:Dot(a:Forward()), -delta:Dot(a:Right()), delta:Dot(a:Up()))
    local tmin, tmax = 0, 1
    for _, ax in ipairs({ "x", "y", "z" }) do
        if math.abs(ld[ax]) < 1e-9 then
            if ls[ax] < mins[ax] or ls[ax] > maxs[ax] then return nil end
        else
            local t1, t2 = (mins[ax] - ls[ax]) / ld[ax], (maxs[ax] - ls[ax]) / ld[ax]
            if t1 > t2 then t1, t2 = t2, t1 end
            tmin, tmax = math.max(tmin, t1), math.min(tmax, t2)
            if tmin > tmax then return nil end
        end
    end
    return start + delta * tmin, Vector(), tmin
end
function util.IntersectRayWithPlane(start, dir, origin, normal)
    local denom = dir:Dot(normal)
    if math.abs(denom) < 1e-9 then return nil end
    local t = (origin - start):Dot(normal) / denom
    if t < 0 then return nil end
    return start + dir * t
end
function util.IsPointInCone(point, origin, dir, cosAngle, dist)
    local to = point - origin
    local l = to:Length()
    return l <= dist and l > 0 and to:Dot(dir) / l >= cosAngle
end
function util.IsBoxIntersectingSphere(mins, maxs, center, radius)
    local c = Vector(math.Clamp(center.x, mins.x, maxs.x), math.Clamp(center.y, mins.y, maxs.y), math.Clamp(center.z, mins.z, maxs.z))
    return c:DistToSqr(center) <= radius * radius
end
function util.IsOBBIntersectingOBB(o1, a1, mins1, maxs1, o2, a2, mins2, maxs2)
    local r1 = (maxs1 - mins1):Length() / 2
    local r2 = (maxs2 - mins2):Length() / 2
    return o1:Distance(o2) <= r1 + r2
end
function util.IsRayIntersectingRay() return false end
function util.IsSphereIntersectingCone(c, r, origin, dir, cosAngle) return util.IsPointInCone(c, origin, dir, cosAngle, 1e9) end
function util.IsSphereIntersectingSphere(c1, r1, c2, r2) return c1:DistToSqr(c2) <= (r1 + r2) ^ 2 end
function util.IsSkyboxVisibleFromPoint() return false end
function util.CRC(s)
    local crc = 0xFFFFFFFF
    for i = 1, #s do
        crc = bit32.bxor(crc, s:byte(i))
        for _ = 1, 8 do
            local mask = bit32.band(crc, 1) == 1 and 0xEDB88320 or 0
            crc = bit32.bxor(bit32.rshift(crc, 1), mask)
        end
    end
    return tostring(bit32.bxor(crc, 0xFFFFFFFF))
end
local b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
function util.Base64Encode(s, inline)
    local out = {}
    for i = 1, #s, 3 do
        local a, b, c = s:byte(i, i + 2)
        local n = a * 65536 + (b or 0) * 256 + (c or 0)
        local c1, c2, c3, c4 = bit32.rshift(n, 18), bit32.band(bit32.rshift(n, 12), 63), bit32.band(bit32.rshift(n, 6), 63), bit32.band(n, 63)
        out[#out + 1] = b64:sub(c1 + 1, c1 + 1) .. b64:sub(c2 + 1, c2 + 1) .. (b and b64:sub(c3 + 1, c3 + 1) or "=") .. (c and b64:sub(c4 + 1, c4 + 1) or "=")
    end
    return table.concat(out)
end
function util.Base64Decode(s)
    s = s:gsub("[^%w%+/=]", "")
    local out = {}
    local function idx(ch) return (b64:find(ch, 1, true) or 1) - 1 end
    for i = 1, #s, 4 do
        local c1, c2, c3, c4 = s:sub(i, i), s:sub(i + 1, i + 1), s:sub(i + 2, i + 2), s:sub(i + 3, i + 3)
        local n = idx(c1) * 262144 + idx(c2) * 4096 + (c3 ~= "=" and idx(c3) or 0) * 64 + (c4 ~= "=" and idx(c4) or 0)
        out[#out + 1] = string.char(bit32.rshift(n, 16))
        if c3 ~= "=" then out[#out + 1] = string.char(bit32.band(bit32.rshift(n, 8), 255)) end
        if c4 ~= "=" then out[#out + 1] = string.char(bit32.band(n, 255)) end
    end
    return table.concat(out)
end
-- JSON (GMod's TableToJSON: arrays for sequential tables, Vector/Angle as "[x y z]"/"{p y r}")
local function json_escape(s)
    return '"' .. s:gsub('[%c"\\]', function(c)
        local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t", ["\b"] = "\\b", ["\f"] = "\\f" }
        return map[c] or string.format("\\u%04x", c:byte())
    end) .. '"'
end
local function to_json(v, pretty, depth, done)
    local t = type(v)
    if t == "nil" then return "null"
    elseif t == "boolean" then return tostring(v)
    elseif t == "number" then if v ~= v or v == math.huge or v == -math.huge then return "null" end return (v % 1 == 0 and tostring(v) or string.format("%.14g", v))
    elseif t == "string" then return json_escape(v)
    elseif t == "table" then
        local mt = getmetatable(v)
        if mt == getmetatable(Vector()) then return json_escape(string.format("[%g %g %g]", v.x, v.y, v.z)) end
        if mt == getmetatable(Angle()) then return json_escape(string.format("{%g %g %g}", v.p, v.y, v.r)) end
        if done[v] then return "null" end
        done[v] = true
        local nl, ind, ind2 = "", "", ""
        if pretty then nl = "\n"; ind = string.rep("\t", depth + 1); ind2 = string.rep("\t", depth) end
        local parts = {}
        if table.IsSequential(v) and #v > 0 then
            for _, x in ipairs(v) do parts[#parts + 1] = ind .. to_json(x, pretty, depth + 1, done) end
            done[v] = nil
            return "[" .. nl .. table.concat(parts, "," .. nl) .. nl .. ind2 .. "]"
        end
        for k, x in pairs(v) do
            if type(k) == "string" or type(k) == "number" then
                parts[#parts + 1] = ind .. json_escape(tostring(k)) .. ":" .. (pretty and " " or "") .. to_json(x, pretty, depth + 1, done)
            end
        end
        done[v] = nil
        return "{" .. nl .. table.concat(parts, "," .. nl) .. nl .. ind2 .. "}"
    end
    return "null"
end
function util.TableToJSON(t, pretty) return to_json(t, pretty, 0, {}) end
local function json_parse(s)
    local pos = 1
    local function ws() pos = s:find("%S", pos) or #s + 1 end
    local parse
    local function str()
        pos += 1
        local out = {}
        while true do
            local c = s:sub(pos, pos)
            if c == "" then error("unterminated string") end
            if c == '"' then pos += 1 break end
            if c == "\\" then
                local e = s:sub(pos + 1, pos + 1)
                local map = { n = "\n", r = "\r", t = "\t", b = "\b", f = "\f", ['"'] = '"', ["\\"] = "\\", ["/"] = "/" }
                if e == "u" then
                    out[#out + 1] = utf8.char(tonumber(s:sub(pos + 2, pos + 5), 16) or 63)
                    pos += 6
                else
                    out[#out + 1] = map[e] or e
                    pos += 2
                end
            else
                out[#out + 1] = c
                pos += 1
            end
        end
        return table.concat(out)
    end
    parse = function()
        ws()
        local c = s:sub(pos, pos)
        if c == "{" then
            pos += 1
            local t = {}
            ws()
            if s:sub(pos, pos) == "}" then pos += 1 return t end
            while true do
                ws()
                local k = str()
                ws()
                if s:sub(pos, pos) ~= ":" then error("expected :") end
                pos += 1
                local v = parse()
                t[tonumber(k) or k] = v
                ws()
                local d = s:sub(pos, pos)
                pos += 1
                if d == "}" then return t end
                if d ~= "," then error("expected , or }") end
            end
        elseif c == "[" then
            pos += 1
            local t = {}
            ws()
            if s:sub(pos, pos) == "]" then pos += 1 return t end
            while true do
                t[#t + 1] = parse()
                ws()
                local d = s:sub(pos, pos)
                pos += 1
                if d == "]" then return t end
                if d ~= "," then error("expected , or ]") end
            end
        elseif c == '"' then
            local v = str()
            local x, y, z = v:match("^%[([%-%d%.e]+) ([%-%d%.e]+) ([%-%d%.e]+)%]$")
            if x then return Vector(tonumber(x), tonumber(y), tonumber(z)) end
            local p, yy, r = v:match("^{([%-%d%.e]+) ([%-%d%.e]+) ([%-%d%.e]+)}$")
            if p then return Angle(tonumber(p), tonumber(yy), tonumber(r)) end
            return v
        elseif s:sub(pos, pos + 3) == "true" then pos += 4 return true
        elseif s:sub(pos, pos + 4) == "false" then pos += 5 return false
        elseif s:sub(pos, pos + 3) == "null" then pos += 4 return nil
        else
            local num = s:match("^-?%d+%.?%d*[eE]?[-+]?%d*", pos)
            if not num or num == "" then error("unexpected character at " .. pos) end
            pos += #num
            return tonumber(num)
        end
    end
    return parse()
end
function util.JSONToTable(s)
    local ok, r = pcall(json_parse, s)
    if ok and type(r) == "table" then return r end
    return nil
end
function util.KeyValuesToTable(kv)
    -- minimal: "key" "value" pairs with nested { } blocks
    local t = {}
    local stack = { t }
    for line in kv:gmatch("[^\r\n]+") do
        local k, v = line:match('^%s*"([^"]*)"%s+"([^"]*)"')
        if k then stack[#stack][k:lower()] = tonumber(v) or v
        else
            local block = line:match('^%s*"([^"]*)"%s*$')
            if block then local n = {}; stack[#stack][block:lower()] = n; stack[#stack + 1] = n
            elseif line:find("}") then if #stack > 1 then table.remove(stack) end end
        end
    end
    return t
end
function util.TableToKeyValues(t, root)
    local out = { '"' .. (root or "") .. '"', "{" }
    for k, v in pairs(t) do out[#out + 1] = string.format('\t"%s"\t\t"%s"', tostring(k), tostring(v)) end
    out[#out + 1] = "}"
    return table.concat(out, "\n")
end
function util.Compress(s) return s end
function util.Decompress(s) return s end
function util.AddNetworkString(name) return 0 end
function util.NetworkStringToID(name) return 0 end
function util.NetworkIDToString(id) return nil end
function util.DateStamp() return "" end
function util.GetModelInfo() return {} end
function util.GetModelMeshes() return nil end
function util.GetPlayerTrace(ply, dir)
    return { start = ply:EyePos(), endpos = ply:EyePos() + (dir or ply:GetAimVector()) * 56756, filter = ply }
end
function util.GetPData() return "" end
function util.SetPData() end
function util.RemovePData() end
function util.Effect() end
function util.Decal() end
function util.DecalEx() end
function util.DecalMaterial() return "" end
function util.ScreenShake() end
function util.BlastDamage() end
function util.BlastDamageInfo() end
function util.ParticleTracer() end
function util.ParticleTracerEx() end
function util.SpriteTrail() return NULL end
function util.AimVector(viewAngles, fov, x, y, w, h) return viewAngles:Forward() end
function util.TimerCycle() return 0 end
function util.NiceFloat(f) return tostring(math.Round(f, 2)) end
function util.FilterText(s) return s end
function util.tobool(v) return tobool(v) end
function util.IsBinaryModuleInstalled() return false end
function util.MD5(s) return util.CRC(s) end
function util.Stack() return setmetatable({ n = 0, items = {} }, { __index = { Push = function(self, v) self.n += 1; self.items[self.n] = v end, Pop = function(self, n) n = n or 1; local v = self.items[self.n]; for _ = 1, n do self.items[self.n] = nil; self.n = math.max(0, self.n - 1) end return v end, Top = function(self) return self.items[self.n] end, Size = function(self) return self.n end } }) end
function util.worldpicker() return nil end
function util.RelativePathToFull(p) return p end
function util.RelativePathToGMA() return nil end
function util.FullPathToRelative(p) return p end

------------------------------------------------------------------------ globals
function IsValid(v) return type(v) == "table" and v.IsValid ~= nil and v:IsValid() == true end
function IsEntity(v) return isent(v) or (type(v) == "table" and getmetatable(v) == PLY) end
function isentity(v) return IsEntity(v) end
function isvector(v) return type(v) == "table" and getmetatable(v) == getmetatable(Vector()) end
function isangle(v) return type(v) == "table" and getmetatable(v) == getmetatable(Angle()) end
function isnumber(v) return type(v) == "number" end
function isstring(v) return type(v) == "string" end
function istable(v) return type(v) == "table" end
function isfunction(v) return type(v) == "function" end
function isbool(v) return type(v) == "boolean" end
function ispanel(v) return false end
function ismatrix(v) return type(v) == "table" and v.GetTranslation ~= nil end
function tobool(v)
    if v == nil or v == false or v == 0 or v == "0" or v == "false" then return false end
    return true
end
function Either(c, a, b) if c then return a end return b end
function Format(...) return string.format(...) end
function Msg(...) local parts = {}; for i = 1, select("#", ...) do parts[i] = tostring(select(i, ...)) end print(table.concat(parts)) end
MsgN = Msg
MsgC = function(...) local parts = {}; for i = 1, select("#", ...) do local v = select(i, ...); if not IsColor(v) then parts[#parts + 1] = tostring(v) end end print(table.concat(parts)) end
MsgAll = Msg
function ErrorNoHalt(...) local parts = {}; for i = 1, select("#", ...) do parts[i] = tostring(select(i, ...)) end print("[ERROR] " .. table.concat(parts)) end
ErrorNoHaltWithStack = ErrorNoHalt
function PrintMessage(kind, msg) print(msg) end
function PrintTable(t, indent, done)
    indent = indent or 0
    done = done or {}
    done[t] = true
    local keys = table.GetKeys(t)
    table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
    for _, k in ipairs(keys) do
        local v = t[k]
        local pad = string.rep("\t", indent)
        if type(v) == "table" and not done[v] then
            print(pad .. tostring(k) .. ":")
            PrintTable(v, indent + 2, done)
        else
            print(pad .. tostring(k) .. "\t=\t" .. tostring(v))
        end
    end
end
function ProtectedCall(fn, ...) local ok, err = pcall(fn, ...); if not ok then ErrorNoHalt(err) end return ok end
function SortedPairs(t, desc)
    local keys = table.GetKeys(t)
    table.sort(keys, function(a, b) if desc then return tostring(a) > tostring(b) end return tostring(a) < tostring(b) end)
    local i = 0
    return function() i += 1; local k = keys[i]; if k ~= nil then return k, t[k] end end
end
function SortedPairsByValue(t, desc)
    local keys = table.GetKeys(t)
    table.sort(keys, function(a, b) if desc then return t[a] > t[b] end return t[a] < t[b] end)
    local i = 0
    return function() i += 1; local k = keys[i]; if k ~= nil then return k, t[k] end end
end
function SortedPairsByMemberValue(t, member, desc)
    local keys = table.GetKeys(t)
    table.sort(keys, function(a, b) if desc then return t[a][member] > t[b][member] end return t[a][member] < t[b][member] end)
    local i = 0
    return function() i += 1; local k = keys[i]; if k ~= nil then return k, t[k] end end
end
function RandomPairs(t)
    local keys = table.GetKeys(t)
    table.Shuffle(keys)
    local i = 0
    return function() i += 1; local k = keys[i]; if k ~= nil then return k, t[k] end end
end
function include(path) end
function AddCSLuaFile(path) end
function IncludeCS(path) end
function SERVER_() return true end
SERVER = true
CLIENT = false
MENU_DLL = false
GAMEMODE_NAME = "sandbox"
VERSION = 240000
VERSIONSTR = "gmod-rust"
BRANCH = "gmod-rust"
function VGUIFrameTime() return CurTime() end
function UnPredictedCurTime() return CurTime() end
function GetHostName() return (cvar and cvar.get and cvar.get("hostname")) or "" end
function GetGlobalString(k, d) return _G.__globals and _G.__globals[k] or d or "" end
function GetGlobalInt(k, d) return _G.__globals and _G.__globals[k] or d or 0 end
function GetGlobalFloat(k, d) return _G.__globals and _G.__globals[k] or d or 0 end
function GetGlobalBool(k, d) local v = _G.__globals and _G.__globals[k]; if v == nil then return d or false end return v end
function GetGlobalVector(k, d) return _G.__globals and _G.__globals[k] or d or Vector() end
function GetGlobalAngle(k, d) return _G.__globals and _G.__globals[k] or d or Angle() end
function GetGlobalEntity(k, d) return _G.__globals and _G.__globals[k] or d or NULL end
function GetGlobalVar(k, d) local v = _G.__globals and _G.__globals[k]; if v == nil then return d end return v end
local function setg(k, v) _G.__globals = _G.__globals or {}; _G.__globals[k] = v end
SetGlobalString, SetGlobalInt, SetGlobalFloat, SetGlobalBool, SetGlobalVector, SetGlobalAngle, SetGlobalEntity, SetGlobalVar = setg, setg, setg, setg, setg, setg, setg, setg
GetGlobal2String, GetGlobal2Int, GetGlobal2Float, GetGlobal2Bool, GetGlobal2Vector, GetGlobal2Angle, GetGlobal2Entity, GetGlobal2Var = GetGlobalString, GetGlobalInt, GetGlobalFloat, GetGlobalBool, GetGlobalVector, GetGlobalAngle, GetGlobalEntity, GetGlobalVar
SetGlobal2String, SetGlobal2Int, SetGlobal2Float, SetGlobal2Bool, SetGlobal2Vector, SetGlobal2Angle, SetGlobal2Entity, SetGlobal2Var = setg, setg, setg, setg, setg, setg, setg, setg
function TypeID(v)
    local t = type(v)
    if t == "nil" then return TYPE_NIL elseif t == "boolean" then return TYPE_BOOL elseif t == "number" then return TYPE_NUMBER
    elseif t == "string" then return TYPE_STRING elseif t == "function" then return TYPE_FUNCTION elseif t == "table" then
        if isvector(v) then return TYPE_VECTOR elseif isangle(v) then return TYPE_ANGLE elseif IsColor(v) then return TYPE_COLOR elseif IsEntity(v) then return TYPE_ENTITY end
        return TYPE_TABLE
    end
    return TYPE_NONE
end
function DeriveGamemode() end
function AccessorFunc(tab, varname, name, force)
    tab["Get" .. name] = function(self) return self[varname] end
    tab["Set" .. name] = function(self, v)
        if force == FORCE_STRING then v = tostring(v) elseif force == FORCE_NUMBER then v = tonumber(v) elseif force == FORCE_BOOL then v = tobool(v) end
        self[varname] = v
    end
end
function GetConVar_Internal(name) return nil end

------------------------------------------------------------------------ enums used above
TYPE_NONE, TYPE_INVALID, TYPE_NIL, TYPE_BOOL, TYPE_LIGHTUSERDATA, TYPE_NUMBER, TYPE_STRING, TYPE_TABLE, TYPE_FUNCTION, TYPE_USERDATA, TYPE_THREAD = -1, -1, 0, 1, 2, 3, 4, 5, 6, 7, 8
TYPE_ENTITY, TYPE_VECTOR, TYPE_ANGLE, TYPE_PHYSOBJ, TYPE_SAVE, TYPE_RESTORE, TYPE_DAMAGEINFO, TYPE_EFFECTDATA, TYPE_MOVEDATA, TYPE_RECIPIENTFILTER, TYPE_USERCMD = 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19
TYPE_SCRIPTEDVEHICLE, TYPE_MATERIAL, TYPE_PANEL, TYPE_PARTICLE, TYPE_PARTICLEEMITTER, TYPE_TEXTURE, TYPE_USERMSG, TYPE_CONVAR, TYPE_IMESH, TYPE_MATRIX, TYPE_SOUND, TYPE_PIXELVISHANDLE, TYPE_DLIGHT, TYPE_VIDEO, TYPE_FILE, TYPE_LOCOMOTION, TYPE_PATH, TYPE_NAVAREA, TYPE_SOUNDHANDLE, TYPE_NAVLADDER, TYPE_PARTICLESYSTEM, TYPE_PROJECTEDTEXTURE, TYPE_PHYSCOLLIDE, TYPE_SURFACEINFO, TYPE_COUNT, TYPE_COLOR = 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 255
FORCE_STRING, FORCE_NUMBER, FORCE_BOOL, FORCE_ANGLE, FORCE_COLOR, FORCE_VECTOR = 1, 2, 3, 4, 5, 6
SOLID_NONE, SOLID_BSP, SOLID_BBOX, SOLID_OBB, SOLID_OBB_YAW, SOLID_CUSTOM, SOLID_VPHYSICS = 0, 1, 2, 3, 4, 5, 6
COLLISION_GROUP_NONE, COLLISION_GROUP_DEBRIS, COLLISION_GROUP_DEBRIS_TRIGGER, COLLISION_GROUP_INTERACTIVE_DEBRIS, COLLISION_GROUP_INTERACTIVE, COLLISION_GROUP_PLAYER, COLLISION_GROUP_BREAKABLE_GLASS, COLLISION_GROUP_VEHICLE, COLLISION_GROUP_PLAYER_MOVEMENT, COLLISION_GROUP_NPC, COLLISION_GROUP_IN_VEHICLE, COLLISION_GROUP_WEAPON, COLLISION_GROUP_VEHICLE_CLIP, COLLISION_GROUP_PROJECTILE, COLLISION_GROUP_DOOR_BLOCKER, COLLISION_GROUP_PASSABLE_DOOR, COLLISION_GROUP_DISSOLVING, COLLISION_GROUP_PUSHAWAY, COLLISION_GROUP_NPC_ACTOR, COLLISION_GROUP_NPC_SCRIPTED, COLLISION_GROUP_WORLD = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20
CONTENTS_EMPTY, CONTENTS_SOLID, CONTENTS_WINDOW, CONTENTS_AUX, CONTENTS_GRATE, CONTENTS_SLIME, CONTENTS_WATER, CONTENTS_BLOCKLOS, CONTENTS_OPAQUE, CONTENTS_TESTFOGVOLUME, CONTENTS_TEAM4, CONTENTS_TEAM3, CONTENTS_TEAM1, CONTENTS_TEAM2, CONTENTS_IGNORE_NODRAW_OPAQUE, CONTENTS_MOVEABLE, CONTENTS_AREAPORTAL, CONTENTS_PLAYERCLIP, CONTENTS_MONSTERCLIP = 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072
CONTENTS_ORIGIN, CONTENTS_MONSTER, CONTENTS_DEBRIS, CONTENTS_DETAIL, CONTENTS_TRANSLUCENT, CONTENTS_LADDER, CONTENTS_HITBOX = 16777216, 33554432, 67108864, 134217728, 268435456, 536870912, 1073741824
DMG_GENERIC, DMG_CRUSH, DMG_BULLET, DMG_SLASH, DMG_BURN, DMG_VEHICLE, DMG_FALL, DMG_BLAST, DMG_CLUB, DMG_SHOCK, DMG_SONIC, DMG_ENERGYBEAM, DMG_PREVENT_PHYSICS_FORCE, DMG_NEVERGIB, DMG_ALWAYSGIB, DMG_DROWN, DMG_PARALYZE, DMG_NERVEGAS, DMG_POISON, DMG_RADIATION, DMG_DROWNRECOVER, DMG_ACID, DMG_SLOWBURN, DMG_REMOVENORAGDOLL, DMG_PHYSGUN, DMG_PLASMA, DMG_AIRBOAT, DMG_DISSOLVE, DMG_BLAST_SURFACE, DMG_DIRECT, DMG_BUCKSHOT, DMG_SNIPER, DMG_MISSILEDEFENSE = 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608, 16777216, 33554432, 67108864, 134217728, 268435456, 536870912, 1073741824, 2147483648
ACT_RESET, ACT_IDLE, ACT_TRANSITION, ACT_COVER, ACT_COVER_MED, ACT_COVER_LOW, ACT_WALK, ACT_WALK_AIM, ACT_WALK_CROUCH, ACT_WALK_CROUCH_AIM, ACT_RUN, ACT_RUN_AIM, ACT_RUN_CROUCH, ACT_RUN_CROUCH_AIM, ACT_RUN_PROTECTED = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14
HITGROUP_GENERIC, HITGROUP_HEAD, HITGROUP_CHEST, HITGROUP_STOMACH, HITGROUP_LEFTARM, HITGROUP_RIGHTARM, HITGROUP_LEFTLEG, HITGROUP_RIGHTLEG, HITGROUP_GEAR = 0, 1, 2, 3, 4, 5, 6, 7, 10
HUD_PRINTNOTIFY, HUD_PRINTCONSOLE, HUD_PRINTTALK, HUD_PRINTCENTER = 1, 2, 3, 4
MOVETYPE_NONE, MOVETYPE_ISOMETRIC, MOVETYPE_WALK, MOVETYPE_STEP, MOVETYPE_FLY, MOVETYPE_FLYGRAVITY, MOVETYPE_VPHYSICS, MOVETYPE_PUSH, MOVETYPE_NOCLIP, MOVETYPE_LADDER, MOVETYPE_OBSERVER, MOVETYPE_CUSTOM = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11

__glua_classes.Entity = ENT
__glua_classes.NPC = NPC
__glua_classes.Vehicle = VEH
__glua_classes.Weapon = WEP
__glua_classes.Player = PLY
