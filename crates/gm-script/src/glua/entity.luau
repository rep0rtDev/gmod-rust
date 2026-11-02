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
local P = {}
P.__index = P
function P:IsValid() return self.__ent:IsValid() end
function P:GetPos() return self.__ent:GetPos() end
function P:GetAngles() return self.__ent:GetAngles() end
function P:GetVelocity() return self.__ent:GetVelocity() end
function P:SetVelocity(v) self.__ent:SetVelocity(v) end
function P:SetVelocityInstantaneous(v) self.__ent:SetVelocity(v) end
function P:AddVelocity(v) self.__ent:SetVelocity(self.__ent:GetVelocity() + v) end
function P:SetPos(v) self.__ent:SetPos(v) end
function P:SetAngles(a) self.__ent:SetAngles(a) end
function P:EnableMotion(on) qwrite("freeze", self.__ent.__id, not on) end
function P:IsMotionEnabled() local s = snap(self.__ent.__id); return s ~= nil and not s.frozen end
function P:IsMoveable() return self:IsMotionEnabled() end
local function psn(self) return snap(self.__ent.__id) end
function P:IsAsleep() local s = psn(self); return s and s.asleep or false end
function P:Wake() qwrite("set_asleep", self.__ent.__id, false) end
function P:Sleep() qwrite("set_asleep", self.__ent.__id, true) end
function P:GetAngleVelocity() local s = psn(self); return s and Vector(s.ang_velocity.x, s.ang_velocity.y, s.ang_velocity.z) or Vector() end
function P:SetAngleVelocity(w) local t = v3(self.__ent:GetVelocity()); t.angular = v3(w); qwrite("set_velocity", self.__ent.__id, t) end
function P:SetAngleVelocityInstantaneous(w) self:SetAngleVelocity(w) end
function P:AddAngleVelocity(w) self:SetAngleVelocity(self:GetAngleVelocity() + w) end
function P:GetDamping() local s = psn(self); if not s then return 0, 0 end return s.damping_linear, s.damping_angular end
function P:GetSpeedDamping() local l = self:GetDamping(); return l end
function P:GetRotDamping() local _, a = self:GetDamping(); return a end
function P:SetDamping(lin, ang) qwrite("set_damping", self.__ent.__id, { linear = lin, angular = ang }) end
function P:CalculateForceOffset(f, pos) local r = pos - self:GetPos(); return f, r:Cross(f) end
function P:RotateAroundAxis(axis, deg) local a = self:GetAngles(); a:RotateAroundAxis(axis, deg); self:SetAngles(a) end
function P:AlignAngles(from, to)
    local mf, mt, ms = Matrix(), Matrix(), Matrix()
    mf:SetAngles(from); mt:SetAngles(to); ms:SetAngles(self:GetAngles())
    mf:Invert()
    return (mt * mf * ms):GetAngles()
end
function P:EnableGravity(on) qwrite("set_gravity", self.__ent.__id, on) end
function P:EnableCollisions(on) qwrite("set_collide", self.__ent.__id, on) end
function P:EnableDrag(on) end
function P:GetEntity() return self.__ent end
function P:GetMass() local s = psn(self); local m = s and s.mass or 0; if m <= 0 then return 1 end return m end
function P:SetMass(m) qwrite("set_mass", self.__ent.__id, m) end
function P:GetMassCenter() return Vector() end
function P:ApplyForceCenter(f) self.__ent:SetVelocity(self.__ent:GetVelocity() + f / math.max(self:GetMass(), 1e-3) * FrameTime()) end
function P:GetMaterial() return "" end
function P:SetMaterial() end
function P:LocalToWorld(v) return self.__ent:LocalToWorld(v) end
function P:WorldToLocal(v) return self.__ent:WorldToLocal(v) end
function ENT:GetPhysicsObject() return setmetatable({ __ent = self }, P) end
__glua_classes.PhysObj = P

------------------------------------------------------------------------ NPC / Vehicle / Weapon
function NPC:SetEnemy(ply) if ply and ply.IsPlayer and ply:IsPlayer() then qwrite("npc_target", self.__id, { player = ply:UserID() }) else qwrite("npc_target", self.__id, {}) end end
function NPC:SetTarget(ply) self:SetEnemy(ply) end
function NPC:NavSetGoalPos(pos) return qwrite("npc_goto", self.__id, { x = pos.x, y = pos.y, z = pos.z, run = rawget(self, "__vars").__run or false }) end
function NPC:SetLastPosition(pos) self:NavSetGoalPos(pos) end
function NPC:NavSetGoalTarget(ent) if ent.IsPlayer and ent:IsPlayer() then qwrite("npc_chase", self.__id, { player = ent:UserID(), run = rawget(self, "__vars").__run or false }) end end
function NPC:ClearGoal() qwrite("npc_stop", self.__id, nil) end
function NPC:StopMoving() qwrite("npc_stop", self.__id, nil) end
function NPC:SetMovementActivity(act) rawget(self, "__vars").__run = act == ACT_RUN end
function NPC:GetActiveWeapon() local s = snap(self.__id); if s and s.weapon and s.weapon ~= "" then return WEP.__from(s.weapon, self.__id) end return NULL end
function NPC:GetEnemyLastTimeSeen() local s = snap(self.__id); local t = s and s.npc_target_seen or 0; if t == 0 then return 0 end return t * engine.TickInterval() end
function NPC:Give(class) qwrite("npc_weapon", self.__id, class) return WEP.__from(class, self.__id) end
function NPC:SelectWeapon(class) qwrite("npc_weapon", self.__id, class) end
function NPC:DropWeapon() qwrite("npc_weapon", self.__id, "") end
function NPC:GetNPCState() return 0 end
function NPC:IsMoving() local s = snap(self.__id); return s ~= nil and (s.velocity.x ~= 0 or s.velocity.y ~= 0) end
-- Vehicle record from the server (host `vehicle.info`): driver, mass, wheels.
local function vinfo(self) return vehicle.info(self.__id) end
function VEH:GetDriver() local v = vinfo(self); if v and v.driver ~= 0 then return Player(v.driver) end return NULL end
function VEH:GetVehicleParams()
    local v = vinfo(self); if not v then return nil end
    local wheels = {}
    for i, w in ipairs(v.wheels) do wheels[i] = { hub = Vector(w.hub.x, w.hub.y, w.hub.z), radius = w.radius, springConstant = w.spring, springDamping = w.damping, springDampingCompression = w.damping_compression } end
    return { wheelsPerAxle = 2, axleCount = math.ceil(#v.wheels / 2), body = { massOverride = v.mass, addGravity = v.add_gravity }, wheels = wheels, axles = {} }
end
function VEH:GetWheelCount() local v = vinfo(self); return v and #v.wheels or 0 end
function VEH:GetWheelBaseHeight(i) local v = vinfo(self); local w = v and v.wheels[(i or 0) + 1]; return w and (w.hub.z - w.radius) or 0 end
function VEH:GetWheelTotalHeight(i) local v = vinfo(self); local w = v and v.wheels[(i or 0) + 1]; return w and (w.radius * 2) or 0 end
function VEH:GetSpringLength(i) local v = vinfo(self); local w = v and v.wheels[(i or 0) + 1]; return w and w.radius or 0 end
function VEH:IsEngineEnabled() local v = vinfo(self); return v ~= nil and v.driver ~= 0 end
function VEH:IsEngineStarted() return self:IsEngineEnabled() end
function VEH:GetVehicleClass() return self:GetClass() end
function VEH:IsEngineEnabled() return true end
function VEH:GetThrottle() return 0 end
function VEH:GetSteering() return 0 end
function WEP:GetPrintName() return self:GetClass() end
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
-- Buttons of the command the server processed this tick (and the one before, for edges).
function PLY:KeyDown(k) local p = psnap(self.__pid); return p ~= nil and bit.band(p.buttons or 0, k) ~= 0 end
function PLY:KeyDownLast(k) local p = psnap(self.__pid); return p ~= nil and bit.band(p.buttons_last or 0, k) ~= 0 end
function PLY:KeyPressed(k) return self:KeyDown(k) and not self:KeyDownLast(k) end
function PLY:KeyReleased(k) return self:KeyDownLast(k) and not self:KeyDown(k) end
function PLY:IsWalking() return self:KeyDown(IN_WALK) end
function PLY:InVehicle() local p = psnap(self.__pid); return p ~= nil and (p.vehicle or 0) ~= 0 end
function PLY:GetVehicle() local p = psnap(self.__pid); if p and (p.vehicle or 0) ~= 0 then return Entity(p.vehicle) end return NULL end
function PLY:GetDrivingEntity() local p = psnap(self.__pid); if p and (p.driving or 0) ~= 0 then return Entity(p.driving) end return NULL end
function PLY:IsDrivingEntity() local p = psnap(self.__pid); return p ~= nil and (p.driving or 0) ~= 0 end
function PLY:GetUseEntity() local p = psnap(self.__pid); if p and (p.use_target or 0) ~= 0 then return Entity(p.use_target) end return NULL end
function PLY:GetEntityInUse() return self:GetUseEntity() end
function PLY:FlashlightIsOn() local p = psnap(self.__pid); return p ~= nil and p.flashlight == true end
function PLY:Flashlight(on) if on and not self:CanUseFlashlight() then return end qwrite("player.set_flashlight", self.__pid, on == true) end
function PLY:AllowFlashlight(allow) rawset(self, "__flashlight_allowed", allow ~= false) end
function PLY:CanUseFlashlight() local a = rawget(self, "__flashlight_allowed"); return a ~= false end
function PLY:SetFrags(n) qwrite("player.set_frags", self.__pid, math.floor(n)) end
function PLY:SetDeaths(n) qwrite("player.set_deaths", self.__pid, math.floor(n)) end
function PLY:AddFrags(n) self:SetFrags(self:Frags() + math.floor(n)) end
function PLY:AddDeaths(n) self:SetDeaths(self:Deaths() + math.floor(n)) end
function PLY:DropWeapon(w) qwrite("player.drop_weapon", self.__pid, (w and w ~= NULL) and w:GetClass() or nil) end
function PLY:DropNamedWeapon(class) qwrite("player.drop_weapon", self.__pid, class) end
function PLY:DropObject() qwrite("player.drop_object", self.__pid, true) end
function PLY:GetNoTarget() local p = psnap(self.__pid); return p ~= nil and p.no_target == true end
function PLY:SetNoTarget(on) qwrite("player.no_target", self.__pid, on == true) end
function PLY:GetPlayerColor() local p = psnap(self.__pid); local c = p and p.player_color; return c and Vector(c.x, c.y, c.z) or Vector(0, 0, 0) end
function PLY:GetWeaponColor() local p = psnap(self.__pid); local c = p and p.weapon_color; return c and Vector(c.x, c.y, c.z) or Vector(0, 0, 0) end
function PLY:SetPlayerColor(c) qwrite("player.colors", self.__pid, { body = v3(c) }) end
function PLY:SetWeaponColor(c) qwrite("player.colors", self.__pid, { weapon = v3(c) }) end
function PLY:GetMaxArmor() local p = psnap(self.__pid); return p and p.max_armor or 100 end
function PLY:SetMaxArmor(n) qwrite("player.max_armor", self.__pid, math.floor(n)) end
function PLY:Say(text, team) qwrite("player.say", self.__pid, tostring(text)) end
function PLY:Kick(reason) qwrite("player.kick", self.__pid, reason or "") end
function PLY:EnterVehicle(v) if IsValid(v) then qwrite("player.vehicle", self.__pid, v:EntIndex()) end end
function PLY:ExitVehicle() qwrite("player.vehicle", self.__pid, nil) end
function PLY:IsFrozen() local p = psnap(self.__pid); return p ~= nil and p.locked == true end
function PLY:Lock() qwrite("player.lock", self.__pid, true) end
function PLY:UnLock() qwrite("player.lock", self.__pid, false) end
function PLY:IsOnGround() local p = psnap(self.__pid); return p ~= nil and p.on_ground == true end
function PLY:OnGround() return self:IsOnGround() end
function PLY:Crouching() local p = psnap(self.__pid); return p ~= nil and p.ducked == true end
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
function PLY:StripWeapons() qwrite("player.strip_weapon", self.__pid, nil) end
function PLY:StripWeapon(class) qwrite("player.strip_weapon", self.__pid, class) end
-- Ammo: reserve per type from the snapshot (`ammo`), writes through player.set_ammo / give_ammo.
local function ammo_name(t) if type(t) == "number" then return (game and game.GetAmmoName and game.GetAmmoName(t)) or tostring(t) end return tostring(t) end
function PLY:GetAmmoCount(t) local p = psnap(self.__pid); return p and p.ammo and p.ammo[ammo_name(t)] or 0 end
function PLY:GetAmmo() local p = psnap(self.__pid); local out = {}; if p and p.ammo then for k, v in pairs(p.ammo) do out[k] = v end end return out end
function PLY:SetAmmo(count, t) qwrite("player.set_ammo", self.__pid, { type = ammo_name(t), count = math.floor(count) }) end
function PLY:GiveAmmo(count, t, hide) qwrite("player.give_ammo", self.__pid, { type = ammo_name(t), count = math.floor(count) }) return math.floor(count) end
function PLY:RemoveAmmo(count, t) qwrite("player.give_ammo", self.__pid, { type = ammo_name(t), count = -math.floor(count) }) end
function PLY:RemoveAllAmmo() for k in pairs(self:GetAmmo()) do qwrite("player.set_ammo", self.__pid, { type = k, count = 0 }) end end
function PLY:StripAmmo() self:RemoveAllAmmo() end
function WEP:Clip1() local p = psnap(self.__owner); local c = p and p.clips1 and p.clips1[self.__class]; return c or -1 end
function WEP:Clip2() local p = psnap(self.__owner); local c = p and p.clips2 and p.clips2[self.__class]; return c or -1 end
function WEP:SetClip1(n) qwrite("player.set_clip", self.__owner, { class = self.__class, slot = 0, count = math.floor(n) }) end
function WEP:SetClip2(n) qwrite("player.set_clip", self.__owner, { class = self.__class, slot = 1, count = math.floor(n) }) end
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
-- movement tunables are one MoveConfig for the whole server: per-player speed writes stay stubs
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

------------------------------------------------------------------------ Entity: the rest of the documented surface
-- Real where the snapshot or an own op carries it; `__glua_stub` where the engine side (model
-- bones / sequences / flexes, map IO, parenting, render flags, schedules) does not exist yet.
local stub = __glua_stub
local function v3(p) return Vector(p.x, p.y, p.z) end
ENT.GetNWVar = ENT.GetNWVar or ENT.GetNetworkedVar
ENT.SetNWVar = ENT.SetNWVar or ENT.SetNetworkedVar
ENT.GetNWVars = ENT.GetNWVars or function(self) return rawget(self, "__vars").__nw or {} end
-- Local pose: relative to the parent when parented, world otherwise (the game's rule).
function ENT:GetLocalPos() local p = self:GetParent(); if not IsValid(p) then return self:GetPos() end local lp = WorldToLocal(self:GetPos(), self:GetAngles(), p:GetPos(), p:GetAngles()); return lp end
function ENT:GetLocalAngles() local p = self:GetParent(); if not IsValid(p) then return self:GetAngles() end local _, la = WorldToLocal(self:GetPos(), self:GetAngles(), p:GetPos(), p:GetAngles()); return la end
function ENT:SetLocalPos(lp) local p = self:GetParent(); if not IsValid(p) then self:SetPos(lp) return end local wp = LocalToWorld(lp, Angle(), p:GetPos(), p:GetAngles()); self:SetPos(wp) end
function ENT:SetLocalAngles(la) local p = self:GetParent(); if not IsValid(p) then self:SetAngles(la) return end local _, wa = LocalToWorld(Vector(), la, p:GetPos(), p:GetAngles()); self:SetAngles(wa) end
function ENT:GetAbsVelocity() return self:GetVelocity() end
function ENT:SetAbsVelocity(v) self:SetVelocity(v) end
function ENT:GetBaseVelocity() return Vector() end
function ENT:GetLocalAngularVelocity() return Angle() end
function ENT:SetLocalAngularVelocity(a) local ph = self:GetPhysicsObject(); if IsValid(ph) then ph:AddAngleVelocity(Vector(a.p, a.y, a.r)) end end
function ENT:GetNetworkOrigin() return self:GetPos() end
function ENT:SetNetworkOrigin(p) self:SetPos(p) end
function ENT:Alive() return self:Health() > 0 end
function ENT:IsOnGround()
    local p = self:GetPos()
    local tr = util.TraceLine({ start = p + Vector(0, 0, 1), endpos = p - Vector(0, 0, 2), filter = self })
    return tr.Hit
end
ENT.OnGround = ENT.IsOnGround
function ENT:GetGroundEntity()
    local p = self:GetPos()
    local tr = util.TraceLine({ start = p + Vector(0, 0, 1), endpos = p - Vector(0, 0, 2), filter = self })
    if not tr.Hit then return NULL end
    return tr.Entity or game.GetWorld()
end
function ENT:SetGroundEntity(e) end
function ENT:IsInWorld() local p = self:GetPos(); return math.abs(p.x) < 16384 and math.abs(p.y) < 16384 and math.abs(p.z) < 16384 end
function ENT:WaterLevel() return 0 end
function ENT:GetMoveType() local s = snap(self.__id); if not s then return MOVETYPE_NONE end if s.frozen then return MOVETYPE_NONE end return MOVETYPE_VPHYSICS end
function ENT:SetMoveType(t) if t == MOVETYPE_NONE then world.queue("freeze", self.__id, true) elseif t == MOVETYPE_VPHYSICS then world.queue("freeze", self.__id, false) end end
function ENT:GetMoveCollide() return MOVECOLLIDE_DEFAULT end
function ENT:SetMoveCollide(m) end
function ENT:GetSolid() return self:IsValid() and SOLID_VPHYSICS or SOLID_NONE end
function ENT:IsSolid() return self:GetSolid() ~= SOLID_NONE end
function ENT:SetNotSolid(b) world.queue("set_collide", self.__id, not b) end
function ENT:GetSolidFlags() return 0 end
function ENT:SetSolidFlags(f) end
function ENT:AddSolidFlags(f) end
function ENT:RemoveSolidFlags(f) end
function ENT:GetCollisionGroup() return rawget(self, "__cgroup") or COLLISION_GROUP_NONE end
local base_set_cg = ENT.SetCollisionGroup
function ENT:SetCollisionGroup(g)
    rawset(self, "__cgroup", g)
    world.queue("set_collide", self.__id, g ~= COLLISION_GROUP_WORLD and g ~= COLLISION_GROUP_IN_VEHICLE)
end
function ENT:GetGravity() return rawget(self, "__gravity") or 1 end
function ENT:SetGravity(g) rawset(self, "__gravity", g); world.queue("set_gravity", self.__id, g ~= 0) end
function ENT:GetParent() local s = snap(self.__id); if s and s.parent and s.parent ~= 0 then return Entity(s.parent) end return NULL end
function ENT:SetParent(p) if p == nil or p == NULL or not IsValid(p) then qwrite("set_parent", self.__id, nil) else qwrite("set_parent", self.__id, p:EntIndex()) end end
function ENT:SetMoveParent(p) self:SetParent(p) end
function ENT:GetParentWorldTransformMatrix() local p = self:GetParent(); if not IsValid(p) then return nil end local m = Matrix(); m:SetTranslation(p:GetPos()); m:SetAngles(p:GetAngles()); return m end
function ENT:GetMoveParent() return NULL end
function ENT:GetChildren() local out = {}; for _, e in ipairs(ents.GetAll()) do local id = rawget(e, "__id"); local s = id and id ~= 0 and snap(id) or nil; if s and s.parent == self.__id and self.__id ~= 0 then out[#out + 1] = e end end return out end
function ENT:GetParentAttachment() return 0 end
function ENT:GetParentPhysNum() return 0 end
function ENT:GetPhysicsObjectCount() return self:IsValid() and 1 or 0 end
function ENT:GetPhysicsObjectNum(i) return self:GetPhysicsObject() end
function ENT:GetCreationID() return self.__id end
function ENT:MapCreationID() return -1 end
function ENT:CreatedByMap() return false end
function ENT:GetCreationTime() return rawget(self, "__created") or 0 end
function ENT:GetTable() return self end
function ENT:SetTable(t) for k, v in pairs(t) do rawset(self, k, v) end end
function ENT:IsMarkedForDeletion() return rawget(self, "__removing") == true end
function ENT:IsDormant() return false end
function ENT:IsLagCompensated() return false end
function ENT:SetLagCompensated(b) end
function ENT:IsConstraint() return false end
function ENT:IsConstrained() return constraint.HasConstraints(self) end
function ENT:GetConstrainedEntities() local a = {}; for _, c in ipairs(constraint.GetTable(self)) do a[#a + 1] = c.Ent1 == self and c.Ent2 or c.Ent1 end return a end
function ENT:GetConstrainedPhysObjects() local a = {}; for _, e in ipairs(self:GetConstrainedEntities()) do a[#a + 1] = e:GetPhysicsObject() end return a end
function ENT:IsWidget() return false end
function ENT:TestPVS(v) return true end
function ENT:GetWorkshopID() return 0 end
function ENT:GetPersistent() return rawget(self, "__persistent") == true end
function ENT:SetPersistent(b) rawset(self, "__persistent", b) end
function ENT:GetUnFreezable() return rawget(self, "__unfreezable") == true end
function ENT:SetUnFreezable(b) rawset(self, "__unfreezable", b) end
function ENT:GetPhysicsAttacker() return rawget(self, "__attacker") or NULL end
function ENT:SetPhysicsAttacker(p, t) rawset(self, "__attacker", p) end
function ENT:GetCustomCollisionCheck() return rawget(self, "__ccc") == true end
function ENT:SetCustomCollisionCheck(b) rawset(self, "__ccc", b) end
function ENT:EnableCustomCollisions(b) rawset(self, "__ccc", b) end
function ENT:CollisionRulesChanged() end
-- damage: entities through the own health op, players through the player damage op
function ENT:TakeDamage(dmg, attacker, inflictor)
    local d = DamageInfo(); d:SetDamage(dmg); d:SetAttacker(attacker or NULL); d:SetInflictor(inflictor or NULL)
    self:TakeDamageInfo(d)
end
function ENT:TakeDamageInfo(d)
    if self:IsPlayer() then
        local a = d:GetAttacker()
        world.queue("player.damage", self.__id, { amount = d:GetDamage(), attacker = IsValid(a) and a:IsPlayer() and a:UserID() or 0, type = d:GetDamageType() })
        return
    end
    local s = snap(self.__id)
    if not s then return end
    world.queue("set_health", self.__id, math.max(0, s.health - d:GetDamage()))
end
function ENT:DispatchTraceAttack(d, tr, dir) self:TakeDamageInfo(d) end
function ENT:TakePhysicsDamage(d) self:TakeDamageInfo(d) end
function ENT:PassesDamageFilter(d) return true end
function ENT:PassesFilter(caller, e) return true end
function ENT:Visible(other)
    if not IsValid(other) then return false end
    local tr = util.TraceLine({ start = self:EyePos(), endpos = other:EyePos(), filter = { self, other } })
    return not tr.Hit
end
function ENT:VisibleVec(pos)
    local tr = util.TraceLine({ start = self:EyePos(), endpos = pos, filter = self })
    return not tr.Hit
end
function ENT:IsLineOfSightClear(target) if IsEntity(target) then return self:Visible(target) end return self:VisibleVec(target) end
function ENT:DropToFloor()
    local p = self:GetPos()
    local tr = util.TraceLine({ start = p, endpos = p - Vector(0, 0, 4096), filter = self })
    if tr.Hit then self:SetPos(tr.HitPos) end
    return tr.Hit
end
function ENT:PointAtEntity(e) if IsValid(e) then self:SetAngles((e:GetPos() - self:GetPos()):Angle()) end end
function ENT:Use(activator, caller, useType, value) end
function ENT:Respawn() end
function ENT:Dissolve() self:Remove() end
-- removal callbacks and the delete-with chain
local on_remove, delete_with = {}, {}
function ENT:CallOnRemove(name, fn, ...) on_remove[self.__id] = on_remove[self.__id] or {}; on_remove[self.__id][name] = { fn, { ... } } end
function ENT:RemoveCallOnRemove(name) if on_remove[self.__id] then on_remove[self.__id][name] = nil end end
function ENT:DeleteOnRemove(e) delete_with[self.__id] = delete_with[self.__id] or {}; delete_with[self.__id][e.__id] = e end
function ENT:DontDeleteOnRemove(e) if delete_with[self.__id] then delete_with[self.__id][e.__id] = nil end end
if events then
    events.on("entity.removed", function(p)
        local id = p.entity
        local cbs = on_remove[id]
        if cbs then for _, c in pairs(cbs) do c[1](Entity(id), unpack(c[2])) end on_remove[id] = nil end
        local kids = delete_with[id]
        if kids then for _, e in pairs(kids) do if IsValid(e) then e:Remove() end end delete_with[id] = nil end
    end, 1e9)
end
-- NW aliases: NW2 / Networked / Networked2 / DT share the actor-local NW storage
for _, t in ipairs({ "Angle", "Bool", "Entity", "Float", "Int", "String", "Vector" }) do
    local g, s = ENT["GetNW" .. t], ENT["SetNW" .. t]
    ENT["GetNW2" .. t] = g; ENT["SetNW2" .. t] = s
    ENT["GetNetworked" .. t] = g; ENT["SetNetworked" .. t] = s
    ENT["GetNetworked2" .. t] = g; ENT["SetNetworked2" .. t] = s
    ENT["GetDT" .. t] = function(self, i) return self:GetNWVar("dt_" .. t .. i) end
    ENT["SetDT" .. t] = function(self, i, v) self:SetNWVar("dt_" .. t .. i, v) end
end
ENT.SetNetworkedNumber = ENT.SetNWFloat
ENT.GetNetworked2Var, ENT.SetNetworked2Var, ENT.GetNW2Var, ENT.SetNW2Var = ENT.GetNWVar, ENT.SetNWVar, ENT.GetNWVar, ENT.SetNWVar
local nw_proxies = {}
function ENT:SetNWVarProxy(key, fn) nw_proxies[self.__id] = nw_proxies[self.__id] or {}; nw_proxies[self.__id][key] = fn end
function ENT:GetNWVarProxy(key) return nw_proxies[self.__id] and nw_proxies[self.__id][key] end
ENT.SetNW2VarProxy, ENT.GetNW2VarProxy = ENT.SetNWVarProxy, ENT.GetNWVarProxy
ENT.SetNetworkedVarProxy, ENT.GetNetworkedVarProxy = ENT.SetNWVarProxy, ENT.GetNWVarProxy
ENT.SetNetworked2VarProxy, ENT.GetNetworked2VarProxy = ENT.SetNWVarProxy, ENT.GetNWVarProxy
local base_set_nw = ENT.SetNWVar
function ENT:SetNWVar(key, v)
    local old = self:GetNWVar(key)
    base_set_nw(self, key, v)
    local fn = nw_proxies[self.__id] and nw_proxies[self.__id][key]
    if fn and old ~= v then fn(self, key, old, v) end
    if old ~= v then hook.Run("EntityNetworkedVarChanged", self, key, old, v) end
end
ENT.SetNetworkedVar = ENT.SetNWVar
for _, t in ipairs({ "Angle", "Bool", "Entity", "Float", "Int", "String", "Vector" }) do
    ENT["SetNW" .. t] = function(self, k, v) self:SetNWVar(k, v) end
    ENT["SetNW2" .. t], ENT["SetNetworked" .. t], ENT["SetNetworked2" .. t] = ENT["SetNW" .. t], ENT["SetNW" .. t], ENT["SetNW" .. t]
end
-- Player copied the NW setters before the proxy / hook layer existed: take the final ones.
for k, fn in pairs(ENT) do
    if type(fn) == "function" and (k:match("^[GS]etNW") or k:match("^[GS]etNetworked")) then PLY[k] = fn end
end
function ENT:GetNWVarTable() return self:GetNWVars() end
ENT.GetNW2VarTable, ENT.GetNetworkedVarTable, ENT.GetNetworked2VarTable = ENT.GetNWVarTable, ENT.GetNWVarTable, ENT.GetNWVarTable
-- NetworkVar: the scripted-entity data table over the same storage
local dt_callbacks = {}
function ENT:InstallDataTable() rawset(self, "__dt", true) end
function ENT:NetworkVar(t, slot, name, extra)
    if type(slot) == "string" then extra = name; name = slot end
    rawset(self, "Get" .. name, function(s) local v = s:GetNWVar("dt_" .. name); if v == nil and extra and extra.KeyName then return nil end return v end)
    rawset(self, "Set" .. name, function(s, v) s:SetNWVar("dt_" .. name, v) end)
    if extra and extra.KeyName then rawset(self, "__dt_keys", rawget(self, "__dt_keys") or {}); rawget(self, "__dt_keys")[string.lower(extra.KeyName)] = name end
end
function ENT:NetworkVarElement(t, slot, element, name, extra)
    rawset(self, "Get" .. name, function(s) local v = s:GetNWVar("dt_" .. t .. slot); return v and v[element] end)
    rawset(self, "Set" .. name, function(s, v) local cur = s:GetNWVar("dt_" .. t .. slot) or Vector(); cur[element] = v; s:SetNWVar("dt_" .. t .. slot, cur) end)
end
function ENT:NetworkVarNotify(name, fn) self:SetNWVarProxy("dt_" .. name, function(s, k, old, new) fn(s, name, old, new) end) end
function ENT:GetNetworkVars() local o = {}; for k, v in pairs(self:GetNWVars()) do if string.sub(k, 1, 3) == "dt_" then o[string.sub(k, 4)] = v end end return o end
function ENT:RestoreNetworkVars(t) for k, v in pairs(t) do self:SetNWVar("dt_" .. k, v) end end
function ENT:SetNetworkVarsFromMapInput(t) self:RestoreNetworkVars(t) end
function ENT:CallDTVarProxies(t, slot, v) end
function ENT:DTVar(t, slot, name) self:NetworkVar(t, slot, name) end
function ENT:SetNetworkKeyValue(k, v) local n = rawget(self, "__dt_keys") and rawget(self, "__dt_keys")[string.lower(k)]; if n then self["Set" .. n](self, v); return true end return false end
function ENT:SetKeyValue(k, v) hook.Run("EntityKeyValue", self, k, tostring(v)); local si = scripted_ents and scripted_ents.__instance and scripted_ents.__instance(self.__id); if si and si.KeyValue then si:KeyValue(k, tostring(v)) end if not self:SetNetworkKeyValue(k, v) then rawset(self, "__kv", rawget(self, "__kv") or {}); rawget(self, "__kv")[k] = v end end
function ENT:GetKeyValues() return rawget(self, "__kv") or {} end
function ENT:AddCallback(hook, fn) rawset(self, "__cbs", rawget(self, "__cbs") or {}); local c = rawget(self, "__cbs"); c[hook] = c[hook] or {}; table.insert(c[hook], fn); return #c[hook] end
function ENT:RemoveCallback(hook, id) local c = rawget(self, "__cbs"); if c and c[hook] then c[hook][id] = nil end end
function ENT:GetCallbacks(hook) local c = rawget(self, "__cbs"); return c and c[hook] or {} end
function ENT:FireBullets(b)
    if not self:IsPlayer() then return end
    world.queue("fire_bullet", self.__id, { damage = b.Damage or 1, pellets = b.Num or 1, spread = b.Spread and b.Spread.x or 0, force = b.Force or 1 })
end
function ENT:SetUseType(t) rawset(self, "__use_type", t) end
function ENT:GetTouchTrace() return rawget(self, "__touch_trace") or {} end
function ENT:ObjectCaps() return 0 end
function ENT:SetCreator(p) rawset(self, "__creator", p) end
function ENT:GetSpawnFlags() return rawget(self, "__spawnflags") or 0 end
function ENT:HasSpawnFlags(f) return bit.band(self:GetSpawnFlags(), f) ~= 0 end
function ENT:SetSpawnFlags(f) rawset(self, "__spawnflags", f) end
function ENT:AddSpawnFlags(f) rawset(self, "__spawnflags", bit.bor(self:GetSpawnFlags(), f)) end
function ENT:RemoveSpawnFlags(f) rawset(self, "__spawnflags", bit.band(self:GetSpawnFlags(), bit.bnot(f))) end
function ENT:GetFlags() local f = 0; if self:IsOnGround() then f = bit.bor(f, FL_ONGROUND) end if self:IsPlayer() then f = bit.bor(f, FL_CLIENT) elseif self:IsNPC() then f = bit.bor(f, FL_NPC) end if self:GetMoveType() == MOVETYPE_NONE then f = bit.bor(f, FL_FROZEN) end return f end
function ENT:IsFlagSet(f) return bit.band(self:GetFlags(), f) ~= 0 end
function ENT:GetEFlags() return 0 end
function ENT:IsEFlagSet(f) return false end
function ENT:GetBloodColor() return self:IsPlayer() and BLOOD_COLOR_RED or (self:IsNPC() and BLOOD_COLOR_YELLOW or DONT_BLEED) end
function ENT:GetMaterialType() return MAT_DEFAULT end
-- Model data from the server's MDL tables (host `model.info`), cached per model name.
local model_cache = {}
local function minfo(self) local name = self:GetModel(); if not name or name == "" then return nil end local m = model_cache[name]; if m == nil then m = model.info(name) or false; model_cache[name] = m end return m or nil end
function ENT:GetBodyGroups() local m = minfo(self); local out = {}; if m then for i, b in ipairs(m.bodygroups) do out[i] = { id = i - 1, name = b.name, num = b.num, submodels = {} } for k = 0, b.num - 1 do out[i].submodels[k] = tostring(k) end end end return out end
function ENT:GetNumBodyGroups() local m = minfo(self); return m and #m.bodygroups or 0 end
function ENT:GetBodygroupCount(id) local m = minfo(self); local b = m and m.bodygroups[(id or 0) + 1]; return b and b.num or 0 end
function ENT:GetBodygroupName(id) local m = minfo(self); local b = m and m.bodygroups[(id or 0) + 1]; return b and b.name or "" end
function ENT:FindBodygroupByName(name) local m = minfo(self); if m then for i, b in ipairs(m.bodygroups) do if string.lower(b.name) == string.lower(name) then return i - 1 end end end return -1 end
function ENT:SkinCount() local m = minfo(self); return m and m.skins or 0 end
function ENT:GetBoneCount() local m = minfo(self); return m and #m.bones or -1 end
function ENT:GetBoneName(i) local m = minfo(self); return m and m.bones[(i or 0) + 1] or "__INVALIDBONE__" end
function ENT:LookupBone(name) local m = minfo(self); if m then for i, b in ipairs(m.bones) do if b == name then return i - 1 end end end return nil end
function ENT:GetBoneParent(i) local m = minfo(self); local p = m and m.bone_parents[(i or 0) + 1]; return p or -1 end
function ENT:GetSequenceCount() local m = minfo(self); return m and #m.sequences or 0 end
function ENT:GetSequenceName(i) local m = minfo(self); return m and m.sequences[(i or 0) + 1] or "Unknown" end
function ENT:LookupSequence(name) local m = minfo(self); if m then for i, q in ipairs(m.sequences) do if q == name then return i - 1, 0 end end end return -1, 0 end
-- Current sequence from the snapshot (NPCs); durations from the MDL animation tables.
function ENT:GetSequence() local sn = snap(self.__id); local name = sn and sn.sequence; if not name or name == "" then return 0 end local i = self:LookupSequence(name); return i >= 0 and i or 0 end
local function seq_anim(self, i) local m = minfo(self); return m and m.sequence_anim[(i or 0) + 1] end
function ENT:SequenceDuration(i) local a = seq_anim(self, i or self:GetSequence()); if not a or a.fps <= 0 then return 0 end return math.max(a.frames - 1, 1) / a.fps end
function ENT:GetSequenceInfo(i) local m = minfo(self); i = i or self:GetSequence(); local name = m and m.sequences[i + 1]; if not name then return nil end local a = m.sequence_anim[i + 1] or { fps = 0, frames = 0, flags = 0 }; return { label = name, activityname = "", activity = -1, flags = a.flags, numframes = a.frames, fps = a.fps, fadeintime = 0.2, fadeouttime = 0.2, numevents = 0, numblends = 1, anims = { [0] = i } } end
function ENT:GetAnimCount() local m = minfo(self); return m and #m.anims or 0 end
function ENT:GetAnimInfo(i) local m = minfo(self); local a = m and m.anims[(i or 0) + 1]; if not a then return nil end return { label = a.name, fps = a.fps, numframes = a.frames, flags = a.flags } end
function ENT:OBBMins() local mn = self:GetModelBounds(); return mn end
function ENT:OBBMaxs() local _, mx = self:GetModelBounds(); return mx end
function ENT:OBBCenter() local mn, mx = self:GetModelBounds(); return (mn + mx) / 2 end
function ENT:BoundingRadius() return self:GetModelRadius() end
function ENT:GetModelScale() return 1 end
function ENT:GetFriction() local sn = snap(self.__id); if sn and sn.friction then return sn.friction end local m = minfo(self); return m and m.friction or 0 end
function ENT:GetElasticity() local sn = snap(self.__id); if sn and sn.elasticity then return sn.elasticity end local m = minfo(self); return m and m.elasticity or 0 end
function ENT:SetFriction(f) qwrite("set_surface", self.__id, { friction = f }) end
function ENT:SetElasticity(e) qwrite("set_surface", self.__id, { elasticity = e }) end
function ENT:GetRotatedAABB(mins, maxs)
    local a = self:GetAngles()
    local lo, hi = Vector(math.huge, math.huge, math.huge), Vector(-math.huge, -math.huge, -math.huge)
    for _, x in ipairs({ mins.x, maxs.x }) do for _, y in ipairs({ mins.y, maxs.y }) do for _, z in ipairs({ mins.z, maxs.z }) do
        local c = LocalToWorld(Vector(x, y, z), Angle(), Vector(), a)
        lo.x, lo.y, lo.z = math.min(lo.x, c.x), math.min(lo.y, c.y), math.min(lo.z, c.z)
        hi.x, hi.y, hi.z = math.max(hi.x, c.x), math.max(hi.y, c.y), math.max(hi.z, c.z)
    end end end
    return lo, hi
end
function ENT:WorldSpaceAABB() local mn, mx = self:GetModelBounds(); local lo, hi = self:GetRotatedAABB(mn, mx); local p = self:GetPos(); return lo + p, hi + p end
function ENT:NearestPoint(pos) local lo, hi = self:WorldSpaceAABB(); return Vector(math.Clamp(pos.x, lo.x, hi.x), math.Clamp(pos.y, lo.y, hi.y), math.Clamp(pos.z, lo.z, hi.z)) end
function ENT:IsPointInBounds(pos) local lo, hi = self:WorldSpaceAABB(); return pos.x >= lo.x and pos.x <= hi.x and pos.y >= lo.y and pos.y <= hi.y and pos.z >= lo.z and pos.z <= hi.z end
function ENT:GetWorldTransformMatrix() local m = Matrix(); m:SetTranslation(self:GetPos()); m:SetAngles(self:GetAngles()); return m end
function ENT:GetNoDraw() return self:GetRenderMode() == 10 end
function ENT:SetNoDraw(nd) self:SetRenderMode(nd and 10 or 0) end
-- Poser state: the inflator / bone poser / face poser / eye poser tables, readable and writable.
function ENT:GetManipulateBoneScale(b) local sn = snap(self.__id); local v = sn and sn.bone_scales and sn.bone_scales[b]; return v and Vector(v.x, v.y, v.z) or Vector(1, 1, 1) end
function ENT:GetManipulateBoneAngles(b) local sn = snap(self.__id); local v = sn and sn.bone_angles and sn.bone_angles[b]; return v and Angle(v.x, v.y, v.z) or Angle(0, 0, 0) end
function ENT:ManipulateBoneScale(b, sc) qwrite("set_bone_manip", self.__id, { bone = b, scale = v3(sc) }) end
function ENT:ManipulateBoneAngles(b, a) qwrite("set_bone_manip", self.__id, { bone = b, angles = { x = a.p, y = a.y, z = a.r } }) end
function ENT:HasBoneManipulations() local sn = snap(self.__id); return sn ~= nil and ((sn.bone_scales and next(sn.bone_scales) ~= nil) or (sn.bone_angles and next(sn.bone_angles) ~= nil)) == true end
function ENT:GetFlexWeight(i) local sn = snap(self.__id); local w = sn and sn.flex_weights; return w and w[(i or 0) + 1] or 0 end
function ENT:GetFlexScale() local sn = snap(self.__id); return sn and sn.flex_scale or 1 end
function ENT:SetFlexWeight(i, w) qwrite("set_flex", self.__id, { id = i, value = w }) end
function ENT:SetFlexScale(sc) qwrite("set_flex", self.__id, { value = sc }) end
function ENT:HasFlexManipulatior() local sn = snap(self.__id); return sn ~= nil and sn.flex_weights ~= nil end
function ENT:GetEyeTarget() local sn = snap(self.__id); local t = sn and sn.eye_target; return t and Vector(t.x, t.y, t.z) or nil end
function ENT:SetEyeTarget(t) qwrite("set_eye_target", self.__id, t and v3(t) or nil) end
function ENT:GetSequenceList() local m = minfo(self); local out = {}; if m then for i, q in ipairs(m.sequences) do out[i - 1] = q end end return out end
function ENT:GetHitBoxCount(set) local m = minfo(self); return m and m.hitboxes or 0 end
function ENT:GetHitboxSetCount() local m = minfo(self); return m and m.hitbox_sets or 0 end
function ENT:GetFlexNum() local m = minfo(self); return m and #m.flexes or 0 end
function ENT:GetFlexName(i) local m = minfo(self); return m and m.flexes[(i or 0) + 1] or "" end
function ENT:GetFlexIDByName(name) local m = minfo(self); if m then for i, f in ipairs(m.flexes) do if f == name then return i - 1 end end end return nil end
function ENT:GetModelBounds() local m = minfo(self); if not m then return Vector(), Vector() end return Vector(m.mins.x, m.mins.y, m.mins.z), Vector(m.maxs.x, m.maxs.y, m.maxs.z) end
function ENT:GetModelRenderBounds() return self:GetModelBounds() end
function ENT:GetModelRadius() local mn, mx = self:GetModelBounds(); return (mx - mn):Length() / 2 end
function ENT:GetAttachments() local m = minfo(self); local out = {}; if m then for i, a in ipairs(m.attachments) do out[i] = { id = i, name = a.name } end end return out end
function ENT:LookupAttachment(name) local m = minfo(self); if m then for i, a in ipairs(m.attachments) do if a.name == name then return i end end end return 0 end
function ENT:GetAttachment(id) local m = minfo(self); local a = m and m.attachments[id]; if not a then return nil end local pos, ang = LocalToWorld(Vector(a.pos.x, a.pos.y, a.pos.z), Vector(a.forward.x, a.forward.y, a.forward.z):Angle(), self:GetPos(), self:GetAngles()); return { Pos = pos, Ang = ang } end
-- Render state over the prop's own fields (networked to clients like the colour / material stools).
function ENT:GetColor() local s = snap(self.__id); if s and s.color then return Color(s.color.r, s.color.g, s.color.b, s.color.a) end return Color(255, 255, 255, 255) end
function ENT:SetColor(c) c = c or Color(255, 255, 255, 255); qwrite("set_color", self.__id, { r = c.r, g = c.g, b = c.b, a = c.a }) end
function ENT:GetColor4Part() local c = self:GetColor(); return c.r, c.g, c.b, c.a end
function ENT:SetColor4Part(r, g, b, a) qwrite("set_color", self.__id, { r = r, g = g, b = b, a = a }) end
function ENT:GetMaterial() local s = snap(self.__id); return s and s.material or "" end
function ENT:SetMaterial(m) qwrite("set_material", self.__id, m or "") end
function ENT:GetRenderMode() local s = snap(self.__id); return s and s.render_mode or 0 end
function ENT:SetRenderMode(m) qwrite("set_render_mode", self.__id, m) end
function ENT:GetRenderFX() local s = snap(self.__id); return s and s.render_fx or 0 end
function ENT:SetRenderFX(f) qwrite("set_render_fx", self.__id, f) end
function ENT:GetModelContents() return CONTENTS_SOLID end
function ENT:GetSubModels() return {} end
function ENT:Weapon_SetActivity(a, d) end
function ENT:Weapon_TranslateActivity(a) return a end
function ENT:StopLoopingSound(id) end
function ENT:StartLoopingSound(name) return 0 end
function ENT:StopParticles() end
function ENT:RemoveAllDecals() end
function ENT:Input(name, activator, caller, data) local si = scripted_ents and scripted_ents.__instance and scripted_ents.__instance(self.__id); if si and si.AcceptInput and si:AcceptInput(name, activator or NULL, caller or NULL, data) == true then return true end if name == "Kill" then self:Remove() return true end return false end
function ENT:Fire(input, param, delay, activator, caller)
    if (delay or 0) > 0 then local me = self; timer.Simple(delay, function() if IsValid(me) then me:Input(input, activator, caller, param) end end) else self:Input(input, activator, caller, param) end
end
function ENT:ClearAllOutputs() end
function ENT:EmitStepSound() end
FL_ONGROUND, FL_DUCKING, FL_ANIMDUCKING, FL_WATERJUMP, FL_ONTRAIN, FL_INRAIN, FL_FROZEN, FL_ATCONTROLS, FL_CLIENT, FL_FAKECLIENT, FL_INWATER, FL_FLY, FL_SWIM, FL_CONVEYOR, FL_NPC, FL_GODMODE, FL_NOTARGET, FL_AIMTARGET, FL_PARTIALGROUND, FL_STATICPROP, FL_GRAPHED, FL_GRENADE, FL_STEPMOVEMENT, FL_DONTTOUCH, FL_BASEVELOCITY, FL_WORLDBRUSH, FL_OBJECT, FL_KILLME, FL_ONFIRE, FL_DISSOLVING, FL_TRANSRAGDOLL, FL_UNBLOCKABLE_BY_PLAYER = 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608, 16777216, 33554432, 67108864, 134217728, 268435456, 536870912, 1073741824, 2147483648
MOVETYPE_NONE, MOVETYPE_ISOMETRIC, MOVETYPE_WALK, MOVETYPE_STEP, MOVETYPE_FLY, MOVETYPE_FLYGRAVITY, MOVETYPE_VPHYSICS, MOVETYPE_PUSH, MOVETYPE_NOCLIP, MOVETYPE_LADDER, MOVETYPE_OBSERVER, MOVETYPE_CUSTOM = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11
MOVECOLLIDE_DEFAULT, MOVECOLLIDE_FLY_BOUNCE, MOVECOLLIDE_FLY_CUSTOM, MOVECOLLIDE_FLY_SLIDE = 0, 1, 2, 3
SOLID_NONE, SOLID_BSP, SOLID_BBOX, SOLID_OBB, SOLID_OBB_YAW, SOLID_CUSTOM, SOLID_VPHYSICS = 0, 1, 2, 3, 4, 5, 6
COLLISION_GROUP_NONE, COLLISION_GROUP_DEBRIS, COLLISION_GROUP_DEBRIS_TRIGGER, COLLISION_GROUP_INTERACTIVE_DEBRIS, COLLISION_GROUP_INTERACTIVE, COLLISION_GROUP_PLAYER, COLLISION_GROUP_BREAKABLE_GLASS, COLLISION_GROUP_VEHICLE, COLLISION_GROUP_PLAYER_MOVEMENT, COLLISION_GROUP_NPC, COLLISION_GROUP_IN_VEHICLE, COLLISION_GROUP_WEAPON, COLLISION_GROUP_VEHICLE_CLIP, COLLISION_GROUP_PROJECTILE, COLLISION_GROUP_DOOR_BLOCKER, COLLISION_GROUP_PASSABLE_DOOR, COLLISION_GROUP_DISSOLVING, COLLISION_GROUP_PUSHAWAY, COLLISION_GROUP_NPC_ACTOR, COLLISION_GROUP_NPC_SCRIPTED, COLLISION_GROUP_WORLD = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20
BLOOD_COLOR_RED, BLOOD_COLOR_YELLOW, BLOOD_COLOR_GREEN, BLOOD_COLOR_MECH, BLOOD_COLOR_ANTLION, BLOOD_COLOR_ZOMBIE, BLOOD_COLOR_ANTLION_WORKER, DONT_BLEED = 0, 1, 2, 3, 4, 5, 6, -1
MAT_DEFAULT = 67
-- model-data, parenting, render and map-IO methods: honest stubs until the engine side exists
for _, name in ipairs({ "AddEFlags", "AddEffects", "AddFlags", "AddGesture", "AddGestureSequence", "AddLayeredSequence", "AddToMotionController", "AlignAngles", "Blocked", "BodyTarget", "BoneHasFlag", "BoneLength", "BoundingRadius", "ClearPoseParameters", "CopyBoneMatrix", "CreateBoneFollowers", "DestroyBoneFollowers", "DrawShadow", "EnableConstraints", "FindBodygroupByName", "FindGestureLayer", "FindGestureSequenceLayer", "FindTransitionSequence", "FollowBone", "ForcePlayerDrop", "FrameAdvance", "GetAnimCount", "GetAnimInfo", "GetAnimTimeInterval", "GetAttachment", "GetAttachments", "GetBodyGroups", "GetBodygroupCount", "GetBodygroupName", "GetBoneContents", "GetBoneController", "GetBoneCount", "GetBoneMatrix", "GetBoneName", "GetBoneParent", "GetBonePosition", "GetBoneSurfaceProp", "GetBrushPlane", "GetBrushPlaneCount", "GetBrushSurfaces", "GetChildBones", "GetColor", "GetColor4Part", "GetCycle", "GetEditingData", "GetEffects", "GetElasticity", "GetFlexBounds", "GetFlexIDByName", "GetFlexName", "GetFlexNum", "GetFlexScale", "GetFlexType", "GetFlexWeight", "GetFriction", "GetGroundSpeedVelocity", "GetHitBoxBone", "GetHitBoxBounds", "GetHitBoxCount", "GetHitBoxGroupCount", "GetHitBoxHitGroup", "GetHitboxSet", "GetHitboxSetCount", "GetLayerCycle", "GetLayerDuration", "GetLayerPlaybackRate", "GetLayerSequence", "GetLayerWeight", "GetLightingOriginEntity", "GetManipulateBoneAngles", "GetManipulateBoneJiggle", "GetManipulateBonePosition", "GetManipulateBoneScale", "GetMaterial", "GetMaterials", "GetModelBounds", "GetModelRadius", "GetModelRenderBounds", "GetModelScale", "GetMomentaryRotButtonPos", "GetNoDraw", "GetNumBodyGroups", "GetNumPoseParameters", "GetParentWorldTransformMatrix", "GetPlaybackRate", "GetPoseParameter", "GetPoseParameterName", "GetPoseParameterRange", "GetPreferredCarryAngles", "GetRagdollOwner", "GetRenderFX", "GetRenderMode", "GetRotatedAABB", "GetSequence", "GetSequenceActivity", "GetSequenceActivityName", "GetSequenceCount", "GetSequenceGroundSpeed", "GetSequenceInfo", "GetSequenceList", "GetSequenceMoveDist", "GetSequenceMoveYaw", "GetSequenceMovement", "GetSequenceName", "GetSequenceVelocity", "GetShouldPlayPickupSound", "GetShouldServerRagdoll", "GetSpawnEffect", "GetSubMaterial", "GetSurroundingBounds", "GetTransmitWithParent", "GetWorldTransformMatrix", "GibBreakClient", "GibBreakServer", "HasBoneManipulations", "HasFlexManipulatior", "HeadTarget", "IsEffectActive", "IsOnFire", "IsPlayingGesture", "IsPointInBounds", "IsSequenceFinished", "LookupAttachment", "LookupBone", "LookupPoseParameter", "LookupSequence", "MakePhysicsObjectAShadow", "ManipulateBoneAngles", "ManipulateBoneJiggle", "ManipulateBonePosition", "ManipulateBoneScale", "MuzzleFlash", "NearestPoint", "OBBMaxs", "OBBMins", "PhysWake", "PhysicsDestroy", "PhysicsFromMesh", "PhysicsInit", "PhysicsInitBox", "PhysicsInitConvex", "PhysicsInitMultiConvex", "PhysicsInitShadow", "PhysicsInitSphere", "PhysicsInitStatic", "PlayScene", "PrecacheGibs", "RagdollSolve", "RagdollStopControlling", "RagdollUpdatePhysics", "RemoveAllGestures", "RemoveEFlags", "RemoveEffects", "RemoveFlags", "RemoveFromMotionController", "RemoveGesture", "RemoveInternalConstraint", "RemoveLayer", "ResetSequence", "ResetSequenceInfo", "RestartGesture", "SelectWeightedSequence", "SelectWeightedSequenceSeeded", "SendViewModelMatchingSequence", "SequenceDuration", "SetAnimation", "SetAttachment", "SetBloodColor", "SetBodyGroups", "SetBoneController", "SetBoneMatrix", "SetCollisionBounds", "SetCollisionBoundsWS", "SetColor", "SetColor4Part", "SetCycle", "SetElasticity", "SetEntity", "SetEyeTarget", "SetFlexScale", "SetFlexWeight", "SetFriction", "SetHitboxSet", "SetIK", "SetLayerAutokill", "SetLayerBlendIn", "SetLayerBlendOut", "SetLayerCycle", "SetLayerDuration", "SetLayerLooping", "SetLayerPlaybackRate", "SetLayerPriority", "SetLayerSequence", "SetLayerWeight", "SetLightingOriginEntity", "SetMaterial", "SetModelName", "SetModelScale", "SetMoveParent", "SetNoDraw", "SetParent", "SetParentPhysNum", "SetPhysConstraintObjects", "SetPlaybackRate", "SetPoseParameter", "SetPreventTransmit", "SetRagdollAng", "SetRagdollBuildFunction", "SetRagdollPos", "SetRenderFX", "SetRenderMode", "SetSequence", "SetShouldPlayPickupSound", "SetShouldServerRagdoll", "SetSpawnEffect", "SetSubMaterial", "SetSurroundingBounds", "SetSurroundingBoundsType", "SetTransmitWithParent", "SetTrigger", "SetWeaponModel", "StartMotionController", "StopMotionController", "TranslateBoneToPhysBone", "TranslatePhysBoneToBone", "UpdateBoneFollowers", "UseClientSideAnimation", "UseTriggerBounds", "ViewModelIndex", "WorldSpaceAABB" }) do
    if ENT[name] == nil then ENT[name] = stub("Entity:" .. name) end
end

------------------------------------------------------------------------ NPC: the rest
-- Our AI has no schedules / conditions / relationships (not ported by policy); what it has is a
-- target, a chase, a path goal and a squad, carried by the snapshot.
function NPC:GetEnemy() local s = snap(self.__id); if s and s.npc_target and s.npc_target ~= 0 then return Player(s.npc_target) end return NULL end
function NPC:GetTarget() local s = snap(self.__id); if s and s.npc_chase and s.npc_chase ~= 0 then return Player(s.npc_chase) end return self:GetEnemy() end
function NPC:GetGoalPos() local s = snap(self.__id); if s and s.npc_goal then return v3(s.npc_goal) end return Vector() end
function NPC:GetGoalTarget() return self:GetTarget() end
function NPC:IsGoalActive() local s = snap(self.__id); return s ~= nil and s.npc_goal ~= nil end
function NPC:GetLastPosition() return rawget(self, "__last_pos") or self:GetGoalPos() end
function NPC:GetPathDistanceToGoal() if not self:IsGoalActive() then return 0 end return (self:GetGoalPos() - self:GetPos()):Length() end
function NPC:GetPathTimeToGoal() local d = self:GetPathDistanceToGoal(); local sp = self:GetVelocity():Length(); if sp < 1 then return 0 end return d / sp end
function NPC:GetCurWaypointPos() return self:GetGoalPos() end
function NPC:GetNextWaypointPos() return self:GetGoalPos() end
function NPC:IsCurWaypointGoal() return true end
function NPC:GetCurGoalType() if not self:IsGoalActive() then return 0 end if IsValid(self:GetTarget()) then return 2 end return 1 end
function NPC:GetViewOffset() return Vector(0, 0, 66) end
function NPC:GetShootPos() return self:GetPos() + Vector(0, 0, 40) end
function NPC:GetEyeDirection() return self:GetAngles():Forward() end
function NPC:GetHeadDirection() return self:GetAngles():Forward() end
function NPC:GetAimVector() local e = self:GetEnemy(); if IsValid(e) then return (e:GetPos() + Vector(0, 0, 36) - self:GetShootPos()):GetNormalized() end return self:GetEyeDirection() end
function NPC:GetEnemyLastKnownPos(e) e = e or self:GetEnemy(); if IsValid(e) then return e:GetPos() end return Vector() end
NPC.GetEnemyLastSeenPos = NPC.GetEnemyLastKnownPos
function NPC:GetKnownEnemies() local e = self:GetEnemy(); if IsValid(e) then return { e } end return {} end
function NPC:GetKnownEnemyCount() return IsValid(self:GetEnemy()) and 1 or 0 end
function NPC:HasEnemyMemory(e) return IsValid(self:GetEnemy()) and (e == nil or e == self:GetEnemy()) end
function NPC:HasEnemyEluded(e) return false end
function NPC:ClearEnemyMemory(e) self:SetEnemy(NULL) end
function NPC:IsInViewCone(v) local p = IsEntity(v) and v:GetPos() or v; return (p - self:GetPos()):GetNormalized():Dot(self:GetEyeDirection()) > 0 end
function NPC:GetSquad() return rawget(self, "__squad") end
function NPC:SetSquad(name) rawset(self, "__squad", name) end
function NPC:IsSquadLeader() return false end
function NPC:GetNearestSquadMember() return NULL end
function NPC:GetStepHeight() return 18 end
function NPC:SetStepHeight(h) end
function NPC:GetHullType() return HULL_HUMAN end
function NPC:SetHullType(t) end
function NPC:SetHullSizeNormal() end
function NPC:GetNavType() return NAV_GROUND end
function NPC:SetNavType(t) end
function NPC:GetMoveVelocity() return self:GetVelocity() end
function NPC:SetMoveVelocity(v) self:SetVelocity(v) end
function NPC:MoveStop() self:StopMoving() end
function NPC:ClearGoal() world.queue("npc_stop", self.__id) end
function NPC:NavSetGoal(pos) self:NavSetGoalPos(pos) end
function NPC:TargetOrder(e) self:SetTarget(e) end
function NPC:MoveOrder(pos) self:NavSetGoalPos(pos) end
function NPC:Classify() local c = self:GetClass(); if string.find(c, "zombie") or string.find(c, "headcrab") then return CLASS_ZOMBIE elseif string.find(c, "antlion") then return CLASS_ANTLION elseif string.find(c, "combine") or string.find(c, "metropolice") or string.find(c, "manhack") or string.find(c, "stalker") then return CLASS_COMBINE elseif string.find(c, "citizen") or string.find(c, "alyx") or string.find(c, "barney") or string.find(c, "kleiner") or string.find(c, "eli") or string.find(c, "mossman") or string.find(c, "monk") or string.find(c, "gman") or string.find(c, "vortigaunt") or string.find(c, "dog") then return CLASS_PLAYER_ALLY end return CLASS_NONE end
function NPC:Disposition(e) local c = self:Classify(); if c == CLASS_PLAYER_ALLY or c == CLASS_NONE then return D_LI end return D_HT end
function NPC:GetWeapon() local w = rawget(self, "__weapon"); if w then return w end return NULL end
function NPC:GetWeapons() local w = self:GetWeapon(); if IsValid(w) then return { w } end return {} end
function NPC:GetActivity() return self:GetVelocity():Length() > 1 and ACT_RUN or ACT_IDLE end
function NPC:IsCrouching() return false end
function NPC:GetFOV() return 360 end
HULL_HUMAN, HULL_SMALL_CENTERED, HULL_WIDE_HUMAN, HULL_TINY, HULL_WIDE_SHORT, HULL_MEDIUM, HULL_TINY_CENTERED, HULL_LARGE, HULL_LARGE_CENTERED, HULL_MEDIUM_TALL = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9
NAV_GROUND, NAV_JUMP, NAV_FLY, NAV_CLIMB = 0, 1, 2, 3
CLASS_NONE, CLASS_PLAYER, CLASS_PLAYER_ALLY, CLASS_PLAYER_ALLY_VITAL, CLASS_ANTLION, CLASS_BARNACLE, CLASS_BULLSEYE, CLASS_CITIZEN_PASSIVE, CLASS_CITIZEN_REBEL, CLASS_COMBINE, CLASS_COMBINE_GUNSHIP, CLASS_CONSCRIPT, CLASS_HEADCRAB, CLASS_MANHACK, CLASS_METROPOLICE, CLASS_MILITARY, CLASS_SCANNER, CLASS_STALKER, CLASS_VORTIGAUNT, CLASS_ZOMBIE, CLASS_PROTOSNIPER, CLASS_MISSILE, CLASS_FLARE, CLASS_EARTH_FAUNA, CLASS_HACKED_ROLLERMINE, CLASS_COMBINE_HUNTER = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25
D_ER, D_HT, D_FR, D_LI, D_NU = 0, 1, 2, 3, 4
ACT_RESET, ACT_IDLE, ACT_TRANSITION, ACT_COVER, ACT_COVER_MED, ACT_COVER_LOW, ACT_WALK, ACT_WALK_AIM, ACT_WALK_CROUCH, ACT_WALK_CROUCH_AIM, ACT_RUN, ACT_RUN_AIM, ACT_RUN_CROUCH, ACT_RUN_CROUCH_AIM, ACT_RUN_PROTECTED = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14
for _, name in ipairs({ "AddEntityRelationship", "AddRelationship", "AdvancePath", "AlertSound", "AutoMovement", "BecomeRagdoll", "CapabilitiesAdd", "CapabilitiesClear", "CapabilitiesGet", "CapabilitiesHas", "CapabilitiesRemove", "ClearBlockingEntity", "ClearCondition", "ClearExpression", "ClearSchedule", "ConditionID", "ConditionName", "DropWeapon", "ExitScriptedSequence", "FearSound", "FoundEnemySound", "GetArrivalActivity", "GetArrivalDirection", "GetArrivalDistance", "GetArrivalSequence", "GetArrivalSpeed", "GetBestSoundHint", "GetBlockingEntity", "GetCurrentSchedule", "GetCurrentWeaponProficiency", "GetEnemyFirstTimeSeen", "GetEnemyLastTimeSeen", "GetExpression", "GetIdealActivity", "GetIdealMoveAcceleration", "GetIdealMoveSpeed", "GetIdealSequence", "GetIdealYaw", "GetLastTimeTookDamageFromEnemy", "GetMaxLookDistance", "GetMinMoveCheckDist", "GetMinMoveStopDist", "GetMoveDelay", "GetMoveInterval", "GetMovementActivity", "GetMovementSequence", "GetTaskStatus", "GetTimeEnemyLastReacquired", "Give", "HasCondition", "HasObstacles", "IdleSound", "IgnoreEnemyUntil", "IsCurrentSchedule", "IsFacingIdealYaw", "IsMoveYawLocked", "IsRunningBehavior", "IsUnforgettable", "IsUnreachable", "LostEnemySound", "MaintainActivity", "MarkEnemyAsEluded", "MarkTookDamageFromEnemy", "MoveClimbExec", "MoveClimbStart", "MoveClimbStop", "MoveGroundStep", "MoveJumpExec", "MoveJumpStart", "MoveJumpStop", "MovePause", "MoveStart", "NavSetRandomGoal", "NavSetWanderGoal", "PickupWeapon", "PlaySentence", "RememberUnreachable", "RemoveIgnoreConditions", "ResetIdealActivity", "ResetMoveCalc", "RunEngineTask", "SelectWeapon", "SentenceStop", "SetActivity", "SetArrivalActivity", "SetArrivalDirection", "SetArrivalDistance", "SetArrivalSequence", "SetArrivalSpeed", "SetCondition", "SetCurrentWeaponProficiency", "SetExpression", "SetFOV", "SetForceCrouch", "SetIdealActivity", "SetIdealSequence", "SetIdealYaw", "SetIdealYawAndUpdate", "SetIgnoreConditions", "SetMaxLookDistance", "SetMaxRouteRebuildTime", "SetMoveDelay", "SetMoveInterval", "SetMoveYawLocked", "SetMovementSequence", "SetNPCState", "SetSchedule", "SetTaskStatus", "SetUnforgettable", "SetViewOffset", "StartEngineTask", "TaskComplete", "TaskFail", "UpdateEnemyMemory", "UpdateTurnActivity", "UpdateYaw", "UseActBusyBehavior", "UseAssaultBehavior", "UseFollowBehavior", "UseFuncTankBehavior", "UseLeadBehavior", "UseNoBehavior" }) do
    if NPC[name] == nil then NPC[name] = stub("NPC:" .. name) end
end


------------------------------------------------------------------------ PhysObj: the rest
local PHYS = __glua_classes.PhysObj
function PHYS:GetIndex() return 0 end
function PHYS:GetInvMass() return 1 / math.max(self:GetMass(), 1e-6) end
function PHYS:GetEnergy() local v = self:GetVelocity():Length(); return 0.5 * self:GetMass() * v * v end
function PHYS:GetVelocityAtPoint(p) return self:GetVelocity() end
function PHYS:GetPositionMatrix() local m = Matrix(); m:SetTranslation(self:GetPos()); m:SetAngles(self:GetAngles()); return m end
function PHYS:LocalToWorldVector(v) local a = self:GetAngles(); return a:Forward() * v.x + a:Right() * -v.y + a:Up() * v.z end
function PHYS:WorldToLocalVector(v) local a = self:GetAngles(); return Vector(v:Dot(a:Forward()), -v:Dot(a:Right()), v:Dot(a:Up())) end
function PHYS:ApplyForceOffset(f, pos) self:ApplyForceCenter(f) end
local base_gravity, base_collide = PHYS.EnableGravity, PHYS.EnableCollisions
function PHYS:EnableGravity(on) rawset(self.__ent, "__pgravity", on); base_gravity(self, on) end
function PHYS:EnableCollisions(on) rawset(self.__ent, "__pcollide", on); base_collide(self, on) end
function PHYS:IsGravityEnabled() local g = rawget(self.__ent, "__pgravity"); if g == nil then return true end return g end
function PHYS:IsCollisionEnabled() local c = rawget(self.__ent, "__pcollide"); if c == nil then return true end return c end
function PHYS:GetName() return self.__ent:GetClass() end
function PHYS:OutputDebugInfo() print(string.format("PhysObj[%d] mass %.2f pos %s vel %s", self.__ent:EntIndex(), self:GetMass(), tostring(self:GetPos()), tostring(self:GetVelocity()))) end
for _, name in ipairs({ "AddAngleVelocity", "AddGameFlag", "AlignAngles", "ApplyTorqueCenter", "CalculateForceOffset", "CalculateVelocityOffset", "ClearGameFlag", "ComputeShadowControl", "GetAABB", "GetAngleVelocity", "GetBuoyancyRatio", "GetContents", "GetDamping", "GetFrictionSnapshot", "GetInertia", "GetInvInertia", "GetMesh", "GetMeshConvexes", "GetRotDamping", "GetShadowAngles", "GetShadowPos", "GetSpeedDamping", "GetStress", "GetSurfaceArea", "GetVolume", "HasGameFlag", "IsDragEnabled", "IsPenetrating", "RecheckCollisionFilter", "RotateAroundAxis", "SetAngleDragCoefficient", "SetAngleVelocity", "SetAngleVelocityInstantaneous", "SetBuoyancyRatio", "SetContents", "SetDamping", "SetDragCoefficient", "SetInertia", "UpdateShadow" }) do
    if PHYS[name] == nil then PHYS[name] = stub("PhysObj:" .. name) end
end

------------------------------------------------------------------------ Player: the rest
-- Movement constants are the engine's measured ones (hull 32x32x72 / 36 ducked, eyes 64 / 28,
-- step 18); setters without a server op stay stubs rather than pretending.
function PLY:GetHull() return Vector(-16, -16, 0), Vector(16, 16, 72) end
function PLY:GetHullDuck() return Vector(-16, -16, 0), Vector(16, 16, 36) end
function PLY:GetViewOffset() return Vector(0, 0, 64) end
function PLY:GetViewOffsetDucked() return Vector(0, 0, 28) end
function PLY:GetCurrentViewOffset() if self:Crouching() then return self:GetViewOffsetDucked() end return self:GetViewOffset() end
function PLY:GetStepSize() return 18 end
function PLY:GetMaxSpeed() if self:IsSprinting() then return self:GetRunSpeed() end return self:GetWalkSpeed() end
function PLY:GetSlowWalkSpeed() return 100 end
function PLY:GetCrouchedWalkSpeed() return 0.3 end
function PLY:GetLadderClimbSpeed() return 200 end
function PLY:GetLaggedMovementValue() return 1 end
function PLY:GetDrivingEntity() return NULL end
function PLY:IsDrivingEntity() return false end
function PLY:GetDrivingMode() return 0 end
function PLY:OwnerSteamID64() if self.SteamID64 then return self:SteamID64() end return util.SteamIDTo64(self:SteamID()) end
function PLY:SetActiveWeapon(w) if IsValid(w) then self:SelectWeapon(w:GetClass()) end end
function PLY:SwitchToDefaultWeapon() local w = self:GetWeapons()[1]; if w then self:SelectWeapon(w:GetClass()) end end
function PLY:PickupWeapon(w) if not IsValid(w) then return false end self:Give(w:GetClass()); w:Remove(); return true end
local frozen_objs = {}
function PLY:AddFrozenPhysicsObject(ent, ph) frozen_objs[self.__pid] = frozen_objs[self.__pid] or {}; table.insert(frozen_objs[self.__pid], ent) end
function PLY:UnfreezePhysicsObjects()
    local n = 0
    for _, e in ipairs(frozen_objs[self.__pid] or {}) do if IsValid(e) then world.queue("freeze", e.__id, false); n = n + 1 end end
    frozen_objs[self.__pid] = nil
    return n
end
function PLY:PhysgunUnfreeze()
    local n = self:UnfreezePhysicsObjects()
    for _, e in ipairs(ents.GetAll()) do
        local s = snap(e.__id)
        if s and s.frozen and s.owner == self.__pid then world.queue("freeze", e.__id, false); n = n + 1 end
    end
    return n
end
local uid_tables = {}
function PLY:UniqueIDTable(key) uid_tables[self.__pid] = uid_tables[self.__pid] or {}; local t = uid_tables[self.__pid]; t[key] = t[key] or {}; return t[key] end
function PLY:DebugInfo() return string.format("%s [%d] %s hp %d", self:Nick(), self:UserID(), tostring(self:GetPos()), self:Health()) end
for _, name in ipairs({ "SetWalkSpeed", "SetRunSpeed", "SetJumpPower", "SetMaxSpeed", "SetSlowWalkSpeed", "SetDuckSpeed", "SetUnDuckSpeed", "SetStepSize",  "AddPlayerOption", "AddVCDSequenceToGestureSlot", "AllowFlashlight", "AllowImmediateDecalPainting", "AnimResetGestureSlot", "AnimRestartGesture", "AnimRestartMainSequence", "AnimSetGestureSequence", "AnimSetGestureWeight", "Ban", "CanUseFlashlight", "CreateRagdoll", "CrosshairDisable", "CrosshairEnable", "DetonateTripmines", "DisableWorldClicking", "DoAnimationEvent", "DoAttackEvent", "DoCustomAnimEvent", "DoReloadEvent", "DoSecondaryAttack", "DrawViewModel", "DrawWorldModel", "DropNamedWeapon", "DropObject", "DropWeapon", "EnterVehicle", "EquipSuit", "ExitLadder", "ExitVehicle", "Flashlight", "FlashlightIsOn", "GetActivity", "GetAllowFullRotation", "GetAllowWeaponsInVehicle", "GetAmmo", "GetAmmoCount", "GetAvoidPlayers", "GetCanWalk", "GetCanZoom", "GetClassID", "GetCurrentCommand", "GetEntityInUse", "GetFlashlightColor", "GetFriendStatus", "GetHands", "GetHoveredWidget", "GetNoCollideWithTeammates", "GetObserverTarget", "GetPlayerColor", "GetPlayerInfo", "GetPreferredCarryAngles", "GetPressedWidget", "GetPreviousWeapon", "GetPunchAngle", "GetRagdollEntity", "GetRenderAngles", "GetSuitPower", "GetTimeoutSeconds", "GetUnDuckSpeed", "GetUseEntity", "GetViewModel", "GetViewPunchAngles", "GetViewPunchVelocity", "GetVoiceVolumeScale", "GetWeaponColor", "GiveAmmo", "IPAddress", "IsMuted", "IsPlayingTaunt", "IsSuitEquipped", "IsTimingOut", "IsVoiceAudible", "IsWalking", "IsWorldClickingDisabled", "KeyDownLast", "KeyPressed", "KeyReleased", "Kick", "LagCompensation", "LastHitGroup", "LimitHit", "LocalEyeAngles", "Lock", "UnLock", "MotionSensorPos", "PacketLoss", "PickupObject", "PlayStepSound", "RemoveAllAmmo", "RemoveAllItems", "RemoveAmmo", "RemoveSuit", "ResetHull", "Say", "ScreenFade", "SendHint", "SetActivity", "SetAllowFullRotation", "SetAllowWeaponsInVehicle", "SetAmmo", "SetAvoidPlayers", "SetCanWalk", "SetCanZoom", "SetClassID", "SetCrouchedWalkSpeed", "SetCurrentViewOffset", "SetDSP", "SetDeaths", "SetDrivingEntity", "SetEyeAngles", "SetFOV", "SetFlashlightColor", "SetFrags", "SetHands", "SetHoveredWidget", "SetHull", "SetHullDuck", "SetLadderClimbSpeed", "SetLaggedMovementValue", "SetLastHitGroup", "SetMaxArmor", "SetMaxSpeed", "SetMuted", "SetNoCollideWithTeammates", "SetNoTarget", "SetObserverMode", "SetPlayerColor", "SetPressedWidget", "SetRenderAngles", "SetSlowWalkSpeed", "SetStepSize", "SetSuitPower", "SetSuppressPickupNotices", "SetUnDuckSpeed", "SetViewEntity", "SetViewOffset", "SetViewOffsetDucked", "SetViewPunchAngles", "SetViewPunchVelocity", "SetVoiceVolumeScale", "SetWeaponColor", "SetupHands", "ShouldDrawLocalPlayer", "ShouldDropWeapon", "ShowProfile", "SimulateGravGunDrop", "SimulateGravGunPickup", "Spectate", "SpectateEntity", "SprayDecal", "SprintDisable", "SprintEnable", "StartSprinting", "StartWalking", "StopSprinting", "StopWalking", "StopZooming", "StripAmmo", "SuppressHint", "TimeConnected", "TraceHullAttack", "TranslateWeaponActivity", "UnSpectate", "ViewPunch", "ViewPunchReset", "VoiceVolume" }) do
    if PLY[name] == nil then PLY[name] = stub("Player:" .. name) end
end

------------------------------------------------------------------------ Weapon: the rest (over the registered SWEP table)
local function wdef(self) return weapons and weapons.Get and weapons.Get(self:GetClass()) or nil end
function WEP:GetWeaponViewModel() local d = wdef(self); return d and d.ViewModel or "" end
function WEP:GetWeaponWorldModel() local d = wdef(self); return d and d.WorldModel or "" end
function WEP:GetWeight() local d = wdef(self); return d and d.Weight or 5 end
function WEP:GetHoldType() local h = rawget(self, "__holdtype"); if h then return h end local d = wdef(self); return d and d.HoldType or "normal" end
function WEP:SetHoldType(h) rawset(self, "__holdtype", h) end
function WEP:AllowsAutoSwitchFrom() local d = wdef(self); if d and d.AutoSwitchFrom ~= nil then return d.AutoSwitchFrom end return true end
function WEP:AllowsAutoSwitchTo() local d = wdef(self); if d and d.AutoSwitchTo ~= nil then return d.AutoSwitchTo end return true end
function WEP:GetPrimaryAmmoType() local d = wdef(self); local a = d and d.Primary and d.Primary.Ammo; return a and game.GetAmmoID and game.GetAmmoID(a) or -1 end
function WEP:GetSecondaryAmmoType() local d = wdef(self); local a = d and d.Secondary and d.Secondary.Ammo; return a and game.GetAmmoID and game.GetAmmoID(a) or -1 end
function WEP:GetDeploySpeed() local d = wdef(self); return d and d.DeploySpeed or 1 end
function WEP:HasAmmo() return self:Clip1() ~= 0 end
function WEP:GetNextPrimaryFire() return rawget(self, "__next1") or 0 end
function WEP:SetNextPrimaryFire(t) rawset(self, "__next1", t) end
function WEP:GetNextSecondaryFire() return rawget(self, "__next2") or 0 end
function WEP:SetNextSecondaryFire(t) rawset(self, "__next2", t) end
function WEP:LastShootTime() return rawget(self, "__lastshot") or 0 end
function WEP:SetLastShootTime(t) rawset(self, "__lastshot", t or CurTime()) end
for _, name in ipairs({ "CallOnClient", "DefaultReload", "GetActivity", "IsWeaponVisible", "SendWeaponAnim", "SetActivity", "SetClip1", "SetClip2" }) do
    if WEP[name] == nil then WEP[name] = stub("Weapon:" .. name) end
end

------------------------------------------------------------------------ Vehicle: the rest
local veh_wheels = { prop_vehicle_jeep = 4, prop_vehicle_jeep_old = 4, prop_vehicle_apc = 4, prop_vehicle_airboat = 0, prop_vehicle_prisoner_pod = 0 }
local veh_boost = { prop_vehicle_jeep = true, prop_vehicle_jeep_old = true }
function VEH:GetSpeed() return self:GetVelocity():Length() end
function VEH:GetHLSpeed() return self:GetVelocity():Length() end
function VEH:GetPassenger(i) if (i or 0) == 0 then return self:GetDriver() end return NULL end
function VEH:IsValidVehicle() return self:IsValid() end
function VEH:IsEngineStarted() return IsValid(self:GetDriver()) end
function VEH:HasBoost() return veh_boost[self:GetClass()] == true end
function VEH:IsBoosting() return false end
function VEH:BoostTimeLeft() return 0 end
function VEH:IsVehicleBodyInWater() return false end
for _, name in ipairs({ "CheckExitPoint", "EnableEngine", "GetAmmo", "GetCameraDistance", "GetMaxSpeed", "GetOperatingParams", "GetPassengerSeatPoint", "GetRPM", "GetSteeringDegrees", "GetThirdPersonMode", "GetVehicleParams", "GetVehicleViewPosition", "GetWheel", "GetWheelBaseHeight", "GetWheelContactPoint", "GetWheelTotalHeight", "HasBrakePedal", "ReleaseHandbrake", "SetBoost", "SetCameraDistance", "SetHandbrake", "SetHasBrakePedal", "SetMaxReverseThrottle", "SetMaxThrottle", "SetSpringLength", "SetSteering", "SetSteeringDegrees", "SetThirdPersonMode", "SetThrottle", "SetVehicleClass", "SetVehicleEntryAnim", "SetVehicleParams", "SetWheelFriction", "StartEngine" }) do
    if VEH[name] == nil then VEH[name] = stub("Vehicle:" .. name) end
end
