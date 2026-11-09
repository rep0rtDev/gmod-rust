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
-- one snapshot per dispatch (`__gen`): an entity's / a player's table is built once per
-- dispatch, not on every read (a player read cost 60 us through world.players() before)
local snap_gen, snap_cache, psnap_cache = -1, {}, {}
local function fresh()
    local gen = __gen or 0
    if gen ~= snap_gen then snap_gen = gen; snap_cache = {}; psnap_cache = {} end
end
local function snap(id)
    if not world then return nil end
    fresh()
    local e = snap_cache[id]
    if e == nil then e = world.get(id) or false; snap_cache[id] = e end
    return e or nil
end

------------------------------------------------------------------------ Entity
local ENT = {}
ENT.__index = function(self, k)
    local sub = rawget(self, "__class_meta")
    if sub then local f = sub[k]; if f ~= nil then return f end end
    local m = rawget(ENT, k)
    if m ~= nil then return m end
    return rawget(self, "__vars")[k]
end
ENT.__newindex = function(self, k, v) rawget(self, "__vars")[k] = v end
ENT.__tostring = function(self)
    if not self:IsValid() then return "[NULL Entity]" end
    return "Entity [" .. self:EntIndex() .. "][" .. self:GetClass() .. "]"
end
ENT.__eq = function(a, b)
    local pa, pb = rawget(a, "__pid"), rawget(b, "__pid")
    if pa ~= nil or pb ~= nil then return pa == pb end
    local id = rawget(a, "__id")
    if id == nil or id ~= rawget(b, "__id") then return false end
    if id ~= 0 then return true end
    return rawget(a, "__class") == rawget(b, "__class") and rawget(a, "__owner") == rawget(b, "__owner")
end
local entities = setmetatable({}, { __mode = "v" })
-- wrappers are weak; the per-entity state they carry must outlive them (an addon stores
-- EntityMods / NW vars on one wrapper and reads them back through another)
local ent_vars, ent_tables = {}, {}
local function wrap(id, class_meta)
    local e = entities[id]
    if e then return e end
    ent_vars[id] = ent_vars[id] or {}
    ent_tables[id] = ent_tables[id] or {}
    e = setmetatable({ __id = id, __vars = ent_vars[id], __class_meta = class_meta, __tables = ent_tables[id] }, ENT)
    entities[id] = e
    return e
end
function __glua_forget_entity(id) entities[id] = nil ent_vars[id] = nil ent_tables[id] = nil end
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
function ENT:IsPlayerHolding()
    local id = self.__id
    if id == nil or id == 0 then return false end
    for _, p in ipairs(world and world.players() or {}) do
        if p.held == id or p.carry == id then return true end
    end
    return false
end
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
function ENT:GetName()
    local n = rawget(self, "__vars").__name
    if n and n ~= "" then return n end
    local s = snap(self.__id)
    return s and s.name or ""
end
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
function ENT:Spawn()
    local pend = rawget(self, "__pending")
    if not pend then return end
    rawset(self, "__pending", nil)
    qwrite("spawn", 0, { physics = pend.physics, class = pend.class ~= "prop_physics" and pend.class or nil, model = pend.model or "", pos = pend.pos or { x = 0, y = 0, z = 0 }, angles = pend.angles, owner = pend.owner or 0, tag = pend.tag })
end
function ENT:Activate() end
-- physics shape of a scripted entity: before Spawn it rides along with the spawn write, after
-- it the server drops the body and rebuilds it with the shape on the next tick
local function physmode(self, shape)
    local pend = rawget(self, "__pending")
    if pend then pend.physics = shape return end
    if (rawget(self, "__id") or 0) > 0 then qwrite("ent.physics", self.__id, shape) end
end
function ENT:PhysicsInit(solid)
    if solid == SOLID_NONE or solid == 0 then physmode(self, { kind = "none" }) else physmode(self, { kind = "model" }) end
    return true
end
function ENT:PhysicsInitBox(mins, maxs)
    physmode(self, { kind = "box", mins = { x = mins.x, y = mins.y, z = mins.z }, maxs = { x = maxs.x, y = maxs.y, z = maxs.z } })
    return true
end
function ENT:PhysicsInitSphere(r) physmode(self, { kind = "sphere", radius = tonumber(r) or 1 }) return true end
function ENT:PhysicsInitStatic() physmode(self, { kind = "static" }) return true end
function ENT:PhysicsInitShadow() physmode(self, { kind = "static" }) return true end
function ENT:PhysicsDestroy() physmode(self, { kind = "none" }) end
function ENT:PhysWake() if (rawget(self, "__id") or 0) > 0 then qwrite("set_asleep", self.__id, false) end end
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
function P:GetSpeedDamping() local s = psn(self); return s and s.damping_linear or 0 end
function P:GetRotDamping() local s = psn(self); return s and s.damping_angular or 0 end
function P:SetInertia(v) qwrite("set_inertia", self.__ent.__id, { x = v.x, y = v.y, z = v.z }) end
-- components the physics does not know (0) divide by 1
local function over_inertia(self, v) local i = self:GetInertia(); local function d(a, b) return a / (b > 0 and b or 1) end; return Vector(d(v.x, i.x), d(v.y, i.y), d(v.z, i.z)) end
-- a torque impulse about the mass centre (body axes) becomes angular velocity through the inertia
function P:ApplyTorqueCenter(t) self:AddAngleVelocity(over_inertia(self, t)) end
function P:CalculateVelocityOffset(f, pos)
    local lin, ang = self:CalculateForceOffset(f, pos)
    return lin / self:GetMass(), over_inertia(self, ang)
end
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
function P:EnableDrag(on) qwrite("enable_drag", self.__ent.__id, { on = on ~= false }) end
function P:IsDragEnabled() local s = psn(self); return s ~= nil and s.drag == true end
function P:GetEntity() return self.__ent end
function P:GetMass() local s = psn(self); local m = s and s.mass or 0; if m <= 0 then return 1 end return m end
function P:SetMass(m) qwrite("set_mass", self.__ent.__id, m) end
function P:ApplyForceCenter(f) self.__ent:SetVelocity(self.__ent:GetVelocity() + f / math.max(self:GetMass(), 1e-3) * FrameTime()) end
function P:GetMaterial() return "" end
function P:SetMaterial() end
function P:LocalToWorld(v) return self.__ent:LocalToWorld(v) end
function P:WorldToLocal(v) return self.__ent:WorldToLocal(v) end
function ENT:GetPhysicsObject() return setmetatable({ __ent = self }, P) end
__glua_classes.PhysObj = P

------------------------------------------------------------------------ NPC / Vehicle / Weapon
function NPC:SetEnemy(ply) if ply and ply.IsPlayer and ply:IsPlayer() then qwrite("npc_target", self.__id, { player = ply:UserID() }) else qwrite("npc_target", self.__id, {}) end end
-- the game stores a remembered position; our AI tracks the real one, so only the target is taken
function NPC:UpdateEnemyMemory(ent, pos) self:SetEnemy(ent) end
function NPC:SetTarget(ply) self:SetEnemy(ply) end
function NPC:NavSetGoalPos(pos) return qwrite("npc_goto", self.__id, { x = pos.x, y = pos.y, z = pos.z, run = rawget(self, "__vars").__run or false }) end
function NPC:SetLastPosition(pos) self:NavSetGoalPos(pos) end
function NPC:NavSetGoalTarget(ent) if ent.IsPlayer and ent:IsPlayer() then qwrite("npc_chase", self.__id, { player = ent:UserID(), run = rawget(self, "__vars").__run or false }) end end
function NPC:ClearGoal() qwrite("npc_stop", self.__id, nil) end
function NPC:StopMoving() qwrite("npc_stop", self.__id, nil) end
function NPC:SetMovementActivity(act) rawget(self, "__vars").__run = act == ACT_RUN end
function NPC:GetActiveWeapon() local s = snap(self.__id); if s and s.weapon and s.weapon ~= "" then return WEP.__from(s.weapon, self.__id) end return NULL end
function NPC:GetMaxLookDistance() local s = snap(self.__id); return s and s.npc_sight or 0 end
function NPC:SetMaxLookDistance(d) qwrite("npc_sight", self.__id, tonumber(d) or 0) end
function NPC:PickupWeapon(w) if not IsValid(w) then return end qwrite("npc_pickup_weapon", self.__id, w:EntIndex()) end
function NPC:GetEnemyFirstTimeSeen() local s = snap(self.__id); local t = s and s.npc_target_first_seen or 0; if t == 0 then return 0 end return t * engine.TickInterval() end
function NPC:GetLastTimeTookDamageFromEnemy() local s = snap(self.__id); local t = s and s.npc_enemy_damage or 0; if t == 0 then return 0 end return t * engine.TickInterval() end
function NPC:NavSetWanderGoal(min, max) qwrite("npc_wander", self.__id, { x = tonumber(min) or 0, y = tonumber(max) or 0, z = 0 }) end
function NPC:NavSetRandomGoal(minDist, dir) local m = tonumber(minDist) or 0; qwrite("npc_wander", self.__id, { x = m, y = m + 512, z = 0 }) end
function NPC:GetEnemyLastTimeSeen() local s = snap(self.__id); local t = s and s.npc_target_seen or 0; if t == 0 then return 0 end return t * engine.TickInterval() end
function NPC:Give(class) qwrite("npc_weapon", self.__id, class) return WEP.__from(class, self.__id) end
function NPC:SelectWeapon(class) qwrite("npc_weapon", self.__id, class) end
function NPC:DropWeapon() qwrite("npc_weapon", self.__id, "") end
function NPC:GetNPCState()
    local s = snap(self.__id)
    if s == nil then return NPC_STATE_INVALID end
    if s.health ~= nil and s.health <= 0 then return NPC_STATE_DEAD end
    if s.npc_ai_state ~= nil and s.npc_ai_state ~= 0 then return s.npc_ai_state end
    if s.npc_target ~= nil and s.npc_target ~= 0 then return NPC_STATE_COMBAT end
    if s.npc_goal ~= nil then return NPC_STATE_ALERT end
    return NPC_STATE_IDLE
end
function NPC:IgnoreEnemyUntil(ent, time)
    if not IsValid(ent) then return end
    local pid = ent.__pid or ent:EntIndex()
    local tick = math.floor((tonumber(time) or 0) / engine.TickInterval() + 0.5)
    qwrite("npc_ignore", self.__id, { x = pid, y = tick, z = 0 })
end
-- unreachable memory over the same ignore list: the game forgets an unreachable enemy after a
-- few seconds (its interval is not measured; 5 s here)
function NPC:RememberUnreachable(ent) self:IgnoreEnemyUntil(ent, CurTime() + 5) end
function NPC:IsUnreachable(ent)
    if not IsValid(ent) then return false end
    local pid = ent.__pid or ent:EntIndex()
    local s = snap(self.__id)
    for _, p in ipairs(s and s.npc_ignore or {}) do if p == pid then return true end end
    return false
end
function NPC:MarkEnemyAsEluded() local e = self:GetEnemy(); if IsValid(e) then self:IgnoreEnemyUntil(e, CurTime()) end end
function NPC:IsMoving() local s = snap(self.__id); return s ~= nil and (s.velocity.x ~= 0 or s.velocity.y ~= 0) end
-- Vehicle record from the server (host `vehicle.info`): driver, mass, wheels.
local function vinfo(self) return vehicle.info(self.__id) end
function VEH:GetDriver() local v = vinfo(self); if v and v.driver ~= 0 then return Player(v.driver) end return NULL end
function VEH:GetVehicleParams()
    local v = vinfo(self); if not v then return nil end
    local wheels = {}
    for i, w in ipairs(v.wheels) do wheels[i] = { hub = Vector(w.hub.x, w.hub.y, w.hub.z), radius = w.radius, springConstant = w.spring, springDamping = w.damping, springDampingCompression = w.damping_compression } end
    local d = v.drive
    local engine = d and { maxSpeed = d.max_fwd / 17.6, maxRevSpeed = d.max_rev / 17.6, axleRatio = d.axle_ratio, gearCount = #d.gears, gearRatio = d.gears, shiftUpRPM = d.shift_up, shiftDownRPM = d.shift_down } or nil
    local steering = d and { degreesSlow = d.deg_slow, degreesFast = d.deg_fast, speedSlow = d.slow_speed / 17.6, speedFast = d.fast_speed / 17.6, steeringExponent = d.steer_exp } or nil
    return { wheelsPerAxle = 2, axleCount = math.ceil(#v.wheels / 2), engine = engine, steering = steering, body = { massOverride = v.mass, addGravity = v.add_gravity }, wheels = wheels, axles = {} }
end
function VEH:GetWheelCount() local v = vinfo(self); return v and #v.wheels or 0 end
function VEH:GetWheelBaseHeight(i) local v = vinfo(self); local w = v and v.wheels[(i or 0) + 1]; return w and (w.hub.z - w.radius) or 0 end
function VEH:GetWheelTotalHeight(i) local v = vinfo(self); local w = v and v.wheels[(i or 0) + 1]; return w and (w.radius * 2) or 0 end
function VEH:GetWheelContactPoint(i)
    local v = vinfo(self); local w = v and v.wheels[(i or 0) + 1]
    if not w or not w.contact then return Vector(0, 0, 0), false end
    return Vector(w.contact.x, w.contact.y, w.contact.z), w.touching == true
end
-- suspension rest length is one value for the car's wheels (the vehicle script has one)
function VEH:GetSpringLength(i) local v = vinfo(self); return v and v.rest or 0 end
function VEH:SetSpringLength(i, len) qwrite("vehicle.tuning", self.__id, { rest = tonumber(len) or 0 }) end
-- throttle caps clamp the driver's pedal in the drive step each tick
function VEH:SetMaxThrottle(t) qwrite("vehicle.tuning", self.__id, { max_throttle = tonumber(t) or 1 }) end
function VEH:SetMaxReverseThrottle(t) qwrite("vehicle.tuning", self.__id, { max_rev_throttle = math.abs(tonumber(t) or 1) }) end
function VEH:GetMaxThrottle() local v = vinfo(self); return v and v.max_throttle or 1 end
function VEH:GetMaxReverseThrottle() local v = vinfo(self); return v and -v.max_rev_throttle or -1 end
-- one steering cone at every speed (the measured spec interpolates slow -> fast)
function VEH:SetSteeringDegrees(d) local x = tonumber(d) or 0; qwrite("vehicle.tuning", self.__id, { deg_slow = x, deg_fast = x }) end
-- tyre friction is one value for the car (the raycast wheels share it); the wheel index is accepted
function VEH:SetWheelFriction(i, f) qwrite("vehicle.tuning", self.__id, { tyre_mu = tonumber(f) or 0 }) end
function VEH:GetWheelFriction(i) local v = vinfo(self); return v and v.tyre_mu or 0 end
-- the subset of the game's vehicle params table our drive model has: steering degrees, engine top
-- speeds (mph -> u/s), axle suspension constants (applied to every wheel) and wheel friction
function VEH:SetVehicleParams(p)
    if type(p) ~= "table" then return end
    local tune = {}
    if type(p.steering) == "table" then
        tune.deg_slow = tonumber(p.steering.degreesSlow)
        tune.deg_fast = tonumber(p.steering.degreesFast)
    end
    if type(p.engine) == "table" then
        if p.engine.maxSpeed then tune.max_fwd = tonumber(p.engine.maxSpeed) * 17.6 end
        if p.engine.maxRevSpeed then tune.max_rev = tonumber(p.engine.maxRevSpeed) * 17.6 end
    end
    if type(p.axles) == "table" then
        local n = self:GetWheelCount()
        for _, ax in ipairs(p.axles) do
            if type(ax.wheels) == "table" and ax.wheels.frictionScale then tune.tyre_mu = tonumber(ax.wheels.frictionScale) end
            if type(ax.suspension) == "table" then
                for i = 0, math.max(n - 1, 0) do
                    qwrite("vehicle.wheel", self.__id, { idx = i, spring = tonumber(ax.suspension.springConstant),
                        damping = tonumber(ax.suspension.springDamping), damping_compression = tonumber(ax.suspension.springDampingCompression) })
                end
            end
        end
    end
    if next(tune) then qwrite("vehicle.tuning", self.__id, tune) end
end
-- seat and view points come from the vehicle model's attachments, as the server's enter path uses them
function VEH:GetPassengerSeatPoint(i)
    local a = self:GetAttachment(self:LookupAttachment("vehicle_feet_passenger0"))
    if a then return a.Pos, a.Ang end
    return self:GetPos(), self:GetAngles()
end
function VEH:GetVehicleViewPosition(role)
    local a = self:GetAttachment(self:LookupAttachment("vehicle_driver_eyes"))
    if a then return a.Pos, a.Ang, 0 end
    return self:GetPos(), self:GetAngles(), 0
end
-- the server's exit search: a 32x32x72 hull from 11.4 u above the origin, clear = exit point
function VEH:CheckExitPoint(yaw, dist)
    local start = self:GetPos() + Vector(0, 0, 11.4)
    local dir = Angle(0, tonumber(yaw) or 0, 0):Forward()
    local endpos = start + dir * (tonumber(dist) or 0)
    local tr = util.TraceHull({ start = start, endpos = endpos, mins = Vector(-16, -16, 0), maxs = Vector(16, 16, 72), filter = self })
    if tr and tr.Hit then return nil end
    return endpos
end
function VEH:GetVehicleClass() return self:GetClass() end
-- driving: the controls live on the server's vehicle (kept until changed), the drive law runs
-- in its tick; readbacks come from the snapshot of the last tick
function VEH:SetThrottle(t) qwrite("vehicle.controls", self.__id, { throttle = tonumber(t) or 0 }) end
function VEH:GetThrottle() local v = vinfo(self); return v and v.throttle or 0 end
function VEH:SetSteering(s, _idx) qwrite("vehicle.controls", self.__id, { steer = tonumber(s) or 0 }) end
function VEH:GetSteering() local v = vinfo(self); return v and v.steer or 0 end
function VEH:SetHandbrake(b) qwrite("vehicle.controls", self.__id, { handbrake = b == true }) end
function VEH:ReleaseHandbrake() self:SetHandbrake(false) end
function VEH:StartEngine(b) qwrite("vehicle.engine", self.__id, b ~= false) end
function VEH:EnableEngine(b) qwrite("vehicle.engine", self.__id, b ~= false) end
function VEH:IsEngineEnabled() local v = vinfo(self); return v ~= nil and v.engine == true end
function VEH:IsEngineStarted() return self:IsEngineEnabled() end
function VEH:GetRPM() local v = vinfo(self); return v and v.rpm or 0 end
function VEH:GetSteeringDegrees() local v = vinfo(self); return v and v.cone or 0 end
function VEH:GetMaxSpeed() local v = vinfo(self); return v and v.drive and v.drive.max_fwd or 0 end
function VEH:GetOperatingParams()
    local v = vinfo(self); if not v then return nil end
    return { speed = v.speed, RPM = v.rpm or 0, gear = v.gear, steeringAngle = v.steer_angle,
        wheelsInContact = v.contacts, isTorqueBoosting = false }
end
-- a game weapon's def (slot, clips, ammo, hold type) or the SWEP table of a scripted one
local function wdef(self)
    local class = self:GetClass()
    local sw = weapons and weapons.GetStored and weapons.GetStored(class)
    if sw then return { slot = sw.Slot or 0, slot_pos = sw.SlotPos or 10, clip1 = sw.Primary and sw.Primary.ClipSize or -1, clip2 = sw.Secondary and sw.Secondary.ClipSize or -1, hold_type = sw.HoldType or "normal", print_name = sw.PrintName } end
    return __weapon_def(class)
end
function WEP:GetPrintName() local d = wdef(self); return d and d.print_name or self:GetClass() end
function WEP:GetMaxClip1() local d = wdef(self); return d and d.clip1 or -1 end
function WEP:GetMaxClip2() local d = wdef(self); return d and d.clip2 or -1 end
function WEP:GetHoldType() local d = wdef(self); return d and d.hold_type or "normal" end
function WEP:GetSlot() local d = wdef(self); return d and d.slot or 0 end
function WEP:GetSlotPos() local d = wdef(self); return d and d.slot_pos or 0 end
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
-- A Player is an Entity: its own methods first, then every Entity method (they read the
-- entity snapshot by __id, which is 0 for a player: no entity, neutral answers), then NW vars.
PLY.__index = function(self, k)
    local m = rawget(PLY, k)
    if m ~= nil then return m end
    local e = rawget(ENT, k)
    if type(e) == "function" then return e end
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
    fresh()
    local p = psnap_cache[id]
    if p == nil then p = world.player(id) or false; psnap_cache[id] = p end
    return p or nil
end
function Player(id)
    if type(id) ~= "number" or id <= 0 then return NULL end
    local p = players[id]
    if p then return p end
    p = setmetatable({ __pid = id, __id = 0, __vars = {}, __tables = {} }, PLY)
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
-- client settings the server knows (the ones the client sends): model choice by the game's table
-- name, skin, colours as "r g b"; anything else is unknown ("" / default)
local function color_cvar(c) return c and string.format("%.2f %.2f %.2f", c.x, c.y, c.z) or "" end
function PLY:GetInfo(cvar)
    local p = psnap(self.__pid)
    if not p then return "" end
    if cvar == "cl_playermodel" then return player_manager.TranslateToPlayerModelName(p.model) or "" end
    if cvar == "cl_playerskin" then return tostring(p.skin or 0) end
    if cvar == "cl_playercolor" then return color_cvar(p.player_color) end
    if cvar == "cl_weaponcolor" then return color_cvar(p.weapon_color) end
    return ""
end
function PLY:GetInfoNum(cvar, default) local v = tonumber(self:GetInfo(cvar)); if v == nil then return default end return v end
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
local function ang3(a) return { x = a.p or a.x or 0, y = a.y or 0, z = a.r or a.z or 0 } end
function PLY:ViewPunch(a) qwrite("player.view_punch", self.__pid, ang3(a)) end
function PLY:SetViewPunchAngles(a) qwrite("player.set_view_punch", self.__pid, ang3(a)) end
function PLY:SetViewPunchVelocity(a) qwrite("player.set_view_punch_vel", self.__pid, ang3(a)) end
function PLY:ViewPunchReset(tolerance) qwrite("player.view_punch_reset", self.__pid, true) end
function PLY:GetViewPunchAngles() local p = psnap(self.__pid); if p == nil or p.punch == nil then return Angle(0, 0, 0) end return Angle(p.punch.x, p.punch.y, p.punch.z) end
PLY.GetPunchAngle = PLY.GetViewPunchAngles
function PLY:GetViewPunchVelocity() local p = psnap(self.__pid); if p == nil or p.punch_vel == nil then return Angle(0, 0, 0) end return Angle(p.punch_vel.x, p.punch_vel.y, p.punch_vel.z) end
function PLY:PickupObject(ent) if not IsValid(ent) then return end qwrite("player.pickup_object", self.__pid, ent:EntIndex()) end
function PLY:PlayStepSound(volume) qwrite("player.step_sound", self.__pid, volume or 1) end
function PLY:ExitLadder() qwrite("player.exit_ladder", self.__pid, true) end
function PLY:SetEyeAngles(a) qwrite("player.set_eye_angles", self.__pid, ang3(a)) end
function PLY:LocalEyeAngles() return self:EyeAngles() end
function PLY:EquipSuit() qwrite("player.set_suit", self.__pid, true) end
function PLY:RemoveSuit() qwrite("player.set_suit", self.__pid, false) end
function PLY:IsSuitEquipped() local p = psnap(self.__pid); return p ~= nil and p.suit == true end
function PLY:SetLastHitGroup(g) qwrite("player.set_last_hit_group", self.__pid, math.floor(tonumber(g) or 0)) end
function PLY:ShouldDropWeapon(b) qwrite("player.should_drop_weapon", self.__pid, b == true) end
function PLY:SetFrags(n) qwrite("player.set_frags", self.__pid, math.floor(n)) end
function PLY:SetDeaths(n) qwrite("player.set_deaths", self.__pid, math.floor(n)) end
function PLY:AddFrags(n) self:SetFrags(self:Frags() + math.floor(n)) end
function PLY:AddDeaths(n) self:SetDeaths(self:Deaths() + math.floor(n)) end
function PLY:DropWeapon(w)
    local wep = (w and w ~= NULL) and w or self:GetActiveWeapon()
    qwrite("player.drop_weapon", self.__pid, (w and w ~= NULL) and w:GetClass() or nil)
    if wep and wep ~= NULL then hook.Run("PlayerDroppedWeapon", self, wep) end
end
function PLY:DropNamedWeapon(class)
    qwrite("player.drop_weapon", self.__pid, class)
    local wep = self:GetWeapon(class)
    if wep and wep ~= NULL then hook.Run("PlayerDroppedWeapon", self, wep) end
end
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
function PLY:Ban(minutes, kick) qwrite("player.ban", self.__pid, nil) end
function PLY:LimitHit(kind) qwrite("player.limit_hit", self.__pid, tostring(kind)) end
-- gesture slots: the server runs one gesture layer per player (slot ids are accepted, not kept)
local function act_name(act) if type(act) == "number" then return util.GetActivityNameByID(act) end return tostring(act) end
function PLY:AnimRestartGesture(slot, act, autokill) qwrite("player.gesture_act", self.__pid, { act = act_name(act), autokill = autokill ~= false }) end
function PLY:AnimSetGestureSequence(slot, seq) local name = type(seq) == "number" and self:GetSequenceName(seq) or tostring(seq); qwrite("player.gesture_set", self.__pid, { sequence = name }) end
function PLY:AnimSetGestureWeight(slot, w) qwrite("player.gesture_set", self.__pid, { weight = tonumber(w) or 1 }) end
function PLY:AnimResetGestureSlot(slot) qwrite("player.gesture_reset", self.__pid, nil) end
local function ply_hold(self) local w = self:GetActiveWeapon(); local h = w and w:IsValid() and w:GetHoldType() or "fist"; return string.upper(h) end
function PLY:DoAnimationEvent(event, data)
    if event == PLAYERANIMEVENT_ATTACK_PRIMARY or event == PLAYERANIMEVENT_ATTACK_SECONDARY then
        self:AnimRestartGesture(GESTURE_SLOT_ATTACK_AND_RELOAD, "ACT_HL2MP_GESTURE_RANGE_ATTACK_" .. ply_hold(self), true)
    elseif event == PLAYERANIMEVENT_RELOAD or event == PLAYERANIMEVENT_RELOAD_LOOP or event == PLAYERANIMEVENT_RELOAD_END then
        self:AnimRestartGesture(GESTURE_SLOT_ATTACK_AND_RELOAD, "ACT_HL2MP_GESTURE_RELOAD_" .. ply_hold(self), true)
    elseif event == PLAYERANIMEVENT_CUSTOM_GESTURE then
        self:AnimRestartGesture(GESTURE_SLOT_CUSTOM, data, true)
    elseif event == PLAYERANIMEVENT_CUSTOM_GESTURE_SEQUENCE then
        self:AnimSetGestureSequence(GESTURE_SLOT_CUSTOM, data)
    elseif event == PLAYERANIMEVENT_CANCEL or event == PLAYERANIMEVENT_CANCEL_RELOAD then
        self:AnimResetGestureSlot(GESTURE_SLOT_ATTACK_AND_RELOAD)
    end
end
function PLY:DoAttackEvent() self:DoAnimationEvent(PLAYERANIMEVENT_ATTACK_PRIMARY) end
function PLY:DoSecondaryAttack() self:DoAnimationEvent(PLAYERANIMEVENT_ATTACK_SECONDARY) end
function PLY:DoReloadEvent() self:DoAnimationEvent(PLAYERANIMEVENT_RELOAD) end
function PLY:DoCustomAnimEvent(event, data) self:DoAnimationEvent(event, data) end
function PLY:RemoveAllItems() self:StripWeapons(); self:RemoveAllAmmo() end
function PLY:EnterVehicle(v) if IsValid(v) then qwrite("player.vehicle", self.__pid, v:EntIndex()) end end
function PLY:ExitVehicle() qwrite("player.vehicle", self.__pid, nil) end
function PLY:IsFrozen() local p = psnap(self.__pid); return p ~= nil and p.locked == true end
function PLY:Lock() qwrite("player.lock", self.__pid, true) end
function PLY:TimeConnected() local p = psnap(self.__pid); if not p then return 0 end return math.max(0, engine.TickCount() - p.joined_tick) * engine.TickInterval() end
function PLY:LastHitGroup() local p = psnap(self.__pid); return p and p.last_hitgroup or 0 end
function PLY:UnLock() qwrite("player.lock", self.__pid, false) end
function PLY:IsOnGround() local p = psnap(self.__pid); return p ~= nil and p.on_ground == true end
function PLY:OnGround() return self:IsOnGround() end
function PLY:Crouching() local p = psnap(self.__pid); return p ~= nil and p.ducked == true end
function PLY:IsSprinting() local p = psnap(self.__pid); if not p then return false end return bit.band(p.buttons or 0, IN_SPEED) ~= 0 and (p.cmd_move.x ~= 0 or p.cmd_move.y ~= 0) end
function PLY:IsTyping() return false end
function PLY:IsSpeaking() return false end
function PLY:Team() return rawget(self, "__vars").__team or 1 end
function PLY:SetTeam(t) rawget(self, "__vars").__team = t end
function PLY:GetEyeTrace()
    return util.TraceLine({ start = self:EyePos(), endpos = self:EyePos() + self:GetAimVector() * 56756, filter = self })
end
PLY.GetEyeTraceNoCursor = PLY.GetEyeTrace
function PLY:Ping() local p = psnap(self.__pid); return math.floor((p and p.ping or 0) + 0.5) end
function PLY:PacketLoss() local p = psnap(self.__pid); return p and p.loss or 0 end
function PLY:GetTimeoutSeconds() local p = psnap(self.__pid); return p and p.timeout or 0 end
function PLY:IsTimingOut() return self:GetTimeoutSeconds() > 1 end
function PLY:IPAddress() local p = psnap(self.__pid); local a = p and p.addr or ""; if a == "" then return "loopback" end return a end
function PLY:IsConnected() return self:IsValid() end
function PLY:IsFullyAuthenticated() return true end
function PLY:IsListenServerHost() return false end
-- writes
function PLY:SetPos(v) qwrite("player.teleport", self.__pid, v3(v)) end
function PLY:SetHealth(h) qwrite("player.set_health", self.__pid, h) end
function PLY:SetArmor(a) qwrite("player.set_armor", self.__pid, a) end
function PLY:Kill() qwrite("player.kill", self.__pid, nil) end
-- @hook GM:PlayerSilentDeath
function PLY:KillSilent() qwrite("player.kill", self.__pid, nil) if hook and hook.Run then hook.Run("PlayerSilentDeath", self) end end
function PLY:Give(class) qwrite("player.give_weapon", self.__pid, class) return WEP.__from(class, self.__pid) end
function PLY:SelectWeapon(class) qwrite("player.select_weapon", self.__pid, class) end
function PLY:TakeDamage(amount, attacker, inflictor)
    qwrite("player.damage", self.__pid, { amount = amount, attacker = attacker and attacker.UserID and attacker:UserID() or 0, type = DMG_GENERIC })
end
function PLY:TakeDamageInfo(info) qwrite("player.damage", self.__pid, { amount = info:GetDamage(), attacker = info:GetAttacker().UserID and info:GetAttacker():UserID() or 0, type = info:GetDamageType() }) end
function PLY:SetVelocity(v) end
function PLY:PrintMessage(kind, msg) qwrite("player.print", self.__pid, tostring(tonumber(kind) or 2) .. "\31" .. tostring(msg)) end
function PLY:ChatPrint(msg) self:PrintMessage(HUD_PRINTTALK, msg) end
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
function PLY:SetPData(k, v) rawget(self, "__vars").__pdata = rawget(self, "__vars").__pdata or {}; rawget(self, "__vars").__pdata[k] = v return true end
function PLY:GetPData(k, d) local pd = rawget(self, "__vars").__pdata; if pd and pd[k] ~= nil then return pd[k] end return d end
function PLY:RemovePData(k) local pd = rawget(self, "__vars").__pdata; if pd then pd[k] = nil end return true end
function PLY:GetCount(kind) local p = psnap(self.__pid); return p and p.counts and p.counts[tostring(kind)] or 0 end
function PLY:CheckLimit(kind) local c = GetConVar("sbox_max" .. tostring(kind)); if not c then return true end local max = c:GetInt(); return max < 0 or self:GetCount(kind) < max end
function PLY:AddCount() end
function PLY:AddCleanup() end
function PLY:GetTool() return nil end
function PLY:IsWorldClicking() return false end
function PLY:UniqueID() return self.__pid end
function PLY:GetViewEntity() return self end
function PLY:GetObserverMode() return 0 end
-- a hint token (#Hint_<name>) the client shows after `delay` seconds
function PLY:SendHint(name, delay) qwrite("player.hint", self.__pid, { name = tostring(name or ""), delay = tonumber(delay) or 0 }) end
-- a full-screen fade on the client (SCREENFADE_IN / OUT / STAYOUT honoured, MODULATE / PURGE not)
function PLY:ScreenFade(flags, color, time, hold)
    color = color or { r = 0, g = 0, b = 0, a = 255 }
    qwrite("player.screen_fade", self.__pid, { flags = tonumber(flags) or 2, r = color.r or 0, g = color.g or 0, b = color.b or 0, a = color.a or 255, time = tonumber(time) or 0, hold = tonumber(hold) or 0 })
end
-- the server-set FOV (Player:SetFOV), else the game's default fov_desired
function PLY:GetFOV() local p = psnap(self.__pid); local f = p and p.fov or 0; if f > 0 then return f end return 75 end
function PLY:SetFOV(fov, time, requester) qwrite("player.fov", self.__pid, { fov = tonumber(fov) or 0, time = tonumber(time) or 0 }) end
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
    if scripted_ents and scripted_ents.__ensure_ai_goal then scripted_ents.__ensure_ai_goal(class) end
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
            ent_vars[p.entity], ent_tables[p.entity] = rawget(e, "__vars"), rawget(e, "__tables")
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
-- ammo types: the game's default table (wiki Default_Ammo_Types) plus what game.AddAmmoType adds
local AMMO = { [1] = "AR2", [2] = "AR2AltFire", [3] = "Pistol", [4] = "SMG1", [5] = "357", [6] = "XBowBolt", [7] = "Buckshot", [8] = "RPG_Round", [9] = "SMG1_Grenade", [10] = "Grenade", [11] = "slam", [12] = "AlyxGun", [13] = "SniperRound", [14] = "SniperPenetratedRound", [15] = "Thumper", [16] = "Gravity", [17] = "Battery", [18] = "GaussEnergy", [19] = "CombineCannon", [20] = "AirboatGun", [21] = "StriderMinigun", [22] = "HelicopterGun", [23] = "9mmRound", [24] = "357Round", [25] = "BuckshotHL1", [26] = "XBowBoltHL1", [27] = "MP5_Grenade", [28] = "RPG_Rocket", [29] = "Uranium", [30] = "GrenadeHL1", [31] = "Hornet", [32] = "Snark", [33] = "TripMine", [34] = "Satchel", [35] = "12mmRound", [36] = "StriderMinigunDirect", [37] = "CombineHeavyCannon" }
local AMMO_DATA = {}
function game.GetAmmoTypes() local t = {}; for id, n in pairs(AMMO) do t[id] = n end return t end
function game.GetAmmoID(name) name = string.lower(tostring(name)); for id, n in pairs(AMMO) do if string.lower(n) == name then return id end end return -1 end
function game.GetAmmoName(id) return AMMO[tonumber(id) or -1] end
function game.AddAmmoType(t)
    if type(t) ~= "table" or type(t.name) ~= "string" then return end
    local id = game.GetAmmoID(t.name)
    if id < 0 then id = #AMMO + 1; AMMO[id] = t.name end
    AMMO_DATA[id] = t
end
function game.GetAmmoData(id)
    id = tonumber(id) or -1
    local name = AMMO[id]; if not name then return nil end
    local d = { id = id, name = name }
    for k, v in pairs(AMMO_DATA[id] or {}) do d[k] = v end
    return d
end
function game.AddDecal() end
function game.AddParticles() end
function game.MountGMA() return false end
function game.GetMapNext() return "" end
function game.GetMapVersion() return __map_version() end
function game.StartSpot() local p = game.__start_spot(); if not p then return Vector() end return Vector(p.x, p.y, p.z) end
function game.MaxPlayers() return 128 end
function game.SetGlobalState() end
function game.GetSunInfo() local d = __sun_dir(); if not d then return nil end return { direction = Vector(d.x, d.y, d.z), obstruction = 0 } end
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
-- addons loaded in this runtime and the mounted content providers
function engine.GetAddons()
    local t = {}
    for _, n in ipairs(__addons()) do t[#t + 1] = { title = n, file = n, downloaded = true, mounted = true, wsid = "0", models = 0, updated = 0, size = 0, tags = "", timeadded = 0 } end
    return t
end
function engine.GetGames()
    local t = {}
    for _, l in ipairs(__mounted()) do t[#t + 1] = { title = l, folder = l, depot = 0, mounted = true, owned = true, installed = true } end
    return t
end
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
function engine.GetMountedContent() local t = {}; for _, l in ipairs(__mounted()) do t[#t + 1] = { title = l, folder = l, mounted = true } end for _, n in ipairs(__addons()) do t[#t + 1] = { title = n, folder = n, mounted = true } end return t end
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
-- the server's hull sweep: the world by the box, props by their hulls grown by the box's extent
function util.TraceHull(data)
    local start, endpos = data.start, data.endpos
    local mins, maxs = data.mins or Vector(), data.maxs or Vector()
    local tr = phys and phys.trace_hull and phys.trace_hull(v3(start), v3(endpos), v3(mins), v3(maxs)) or nil
    local r = trace_result(tr, start, endpos, data.filter)
    if data.output then for k, v in pairs(r) do data.output[k] = v end end
    return r
end
function util.TraceEntity(data, ent) return util.TraceLine(data) end
function util.QuickTrace(origin, dir, filter) return util.TraceLine({ start = origin, endpos = origin + dir, filter = filter }) end
-- surfaceproperties: our index is the position in the sorted material list (the engine's is its load order)
function util.GetSurfacePropName(i) local n = surf.names()[(tonumber(i) or -1) + 1]; return n or "" end
function util.GetSurfaceIndex(name) name = string.lower(tostring(name)); for i, n in ipairs(surf.names()) do if n == name then return i - 1 end end return -1 end
function util.GetSurfaceData(i) local n = surf.names()[(tonumber(i) or -1) + 1]; if not n then return nil end return surf.data(n) end
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
function util.DateStamp() return __glua_datestamp() end
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
local timer_cycle_at = nil
function util.TimerCycle() local now = SysTime(); local d = timer_cycle_at and (now - timer_cycle_at) * 1000 or 0; timer_cycle_at = now; return d end
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
function PrintMessage(kind, msg) qwrite("player.print", 0, tostring(tonumber(kind) or 2) .. "\31" .. tostring(msg)) end
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
-- SERVER / CLIENT / MENU_DLL come from the host (`glua::install`) by the actor's realm
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
function ENT:GetLocalAngularVelocity() local s = snap(self.__id); local v = s and s.ang_velocity; if not v then return Angle() end return Angle(v.x, v.y, v.z) end
function ENT:TestPVS(a, b) local p = type(a) == "table" and a.GetPos and a:GetPos() or a; if not p then return false end local me = self:GetPos(); return __pvs_visible({ x = me.x, y = me.y, z = me.z }, { x = p.x, y = p.y, z = p.z }) end
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
function ENT:WaterLevel()
    if not self:IsPlayer() then return 0 end
    local p = psnap(self.__pid)
    return p and p.water_level or 0
end
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
-- a melee-style hull attack from the player: the hull sweep hits one entity, which takes the damage
-- (util.TraceHull is a line sweep here - documented)
function PLY:TraceHullAttack(start, endpos, mins, maxs, dmg, dmgtype, force, quick)
    local tr = util.TraceHull({ start = start, endpos = endpos, mins = mins, maxs = maxs, filter = self })
    local e = tr and tr.Entity
    if IsValid(e) then
        local d = DamageInfo()
        d:SetDamage(tonumber(dmg) or 0)
        d:SetDamageType(dmgtype or DMG_CLUB)
        d:SetAttacker(self)
        d:SetInflictor(self)
        if force then d:SetDamageForce((endpos - start):GetNormalized() * (tonumber(force) or 0)) end
        d:SetDamagePosition(tr.HitPos)
        e:TakeDamageInfo(d)
        return e
    end
    return NULL
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
        __glua_forget_entity(id)
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
    -- the game raises EntityNetworkedVarChanged only on the client, from the NW2 receive
    -- (luabind.md F43/F51, gap-report #212); the server-side proxy stays
    if CLIENT and old ~= v then hook.Run("EntityNetworkedVarChanged", self, key, old, v) end
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
-- an NPC's map keyvalues the brain reads: squadname (SetSquad) and hintgroup (SetHintGroup)
local NPC_KEYVALUES = { squadname = "npc_ai_squad", hintgroup = "npc_ai_hint_group" }
function ENT:SetKeyValue(k, v) hook.Run("EntityKeyValue", self, k, tostring(v)); local nk = NPC_KEYVALUES[string.lower(k)]; if nk and self:IsNPC() then qwrite(nk, self.__id, { name = tostring(v) }) end local si = scripted_ents and scripted_ents.__instance and scripted_ents.__instance(self.__id); if si and si.KeyValue then si:KeyValue(k, tostring(v)) end if not self:SetNetworkKeyValue(k, v) then rawset(self, "__kv", rawget(self, "__kv") or {}); rawget(self, "__kv")[k] = v end end
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
function ENT:GetEFlags() return rawget(self, "__vars").__eflags or 0 end
function ENT:IsEFlagSet(f) return bit.band(self:GetEFlags(), f) ~= 0 end
function ENT:AddEFlags(f) rawget(self, "__vars").__eflags = bit.bor(self:GetEFlags(), f) end
function ENT:RemoveEFlags(f) rawget(self, "__vars").__eflags = bit.band(self:GetEFlags(), bit.bnot(f)) end
function ENT:GetEffects() return rawget(self, "__vars").__effects or 0 end
function ENT:IsEffectActive(f) return bit.band(self:GetEffects(), f) ~= 0 end
function ENT:AddEffects(f)
    rawget(self, "__vars").__effects = bit.bor(self:GetEffects(), f)
    if bit.band(f, EF_NODRAW) ~= 0 then self:SetNoDraw(true) end
end
function ENT:RemoveEffects(f)
    rawget(self, "__vars").__effects = bit.band(self:GetEffects(), bit.bnot(f))
    if bit.band(f, EF_NODRAW) ~= 0 then self:SetNoDraw(false) end
end
function ENT:GetBloodColor()
    if self:IsPlayer() then return BLOOD_COLOR_RED end
    local e = snap(self.__id)
    if e ~= nil and e.blood ~= nil then return e.blood end
    return DONT_BLEED
end
function ENT:SetBloodColor(c) qwrite("entity.set_blood", self.__id, math.floor(tonumber(c) or -1)) end
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
-- PHY solids of the model (bone per solid, solid order): a ragdoll has one body per solid
function ENT:GetPhysicsObjectCount() if not self:IsValid() then return 0 end local m = minfo(self); return math.max(1, m and #(m.phys_bones or {}) or 1) end
function ENT:TranslatePhysBoneToBone(i)
    local m = minfo(self)
    local name = m and m.phys_bones and m.phys_bones[(i or 0) + 1]
    if not name then return -1 end
    for b, n in ipairs(m.bones) do if string.lower(n) == string.lower(name) then return b - 1 end end
    return -1
end
function ENT:TranslateBoneToPhysBone(b)
    local m = minfo(self)
    if not m or not m.phys_bones then return -1 end
    local bone = b or 0
    while bone >= 0 and m.bones[bone + 1] do
        local n = string.lower(m.bones[bone + 1])
        for i, pb in ipairs(m.phys_bones) do if string.lower(pb) == n then return i - 1 end end
        bone = m.bone_parents[bone + 1] or -1
    end
    return -1
end
function ENT:GetSequenceCount() local m = minfo(self); return m and #m.sequences or 0 end
function ENT:GetSequenceName(i) local m = minfo(self); return m and m.sequences[(i or 0) + 1] or "Unknown" end
function ENT:LookupSequence(name) local m = minfo(self); if m then for i, q in ipairs(m.sequences) do if q == name then return i - 1, 0 end end end return -1, 0 end
-- Current sequence from the snapshot (NPCs); durations from the MDL animation tables.
function ENT:GetSequence() local sn = snap(self.__id); local name = sn and sn.sequence; if not name or name == "" then return 0 end local i = self:LookupSequence(name); return i >= 0 and i or 0 end
local function seq_anim(self, i) local m = minfo(self); return m and m.sequence_anim[(i or 0) + 1] end
function ENT:SequenceDuration(i) local a = seq_anim(self, i or self:GetSequence()); if not a or a.fps <= 0 then return 0 end return math.max(a.frames - 1, 1) / a.fps end
local function seq_move(self, i) local m = minfo(self); local v = m and m.sequence_move and m.sequence_move[(i or 0) + 1]; return v and Vector(v.x, v.y, v.z) or Vector() end
function ENT:GetSequenceMoveDist(i) return seq_move(self, i or self:GetSequence()):Length() end
function ENT:GetSequenceMovement(i, s, e) local mv = seq_move(self, i or self:GetSequence()); local f = (tonumber(e) or 1) - (tonumber(s) or 0); return mv:Length() > 0, mv * f, Angle() end
function ENT:GetSequenceVelocity(i, cycle) i = i or self:GetSequence(); local d = self:SequenceDuration(i); if d <= 0 then return Vector() end return seq_move(self, i) / d end
function ENT:GetSequenceGroundSpeed(i) return self:GetSequenceVelocity(i or self:GetSequence()):Length() end
function ENT:GetSequenceMoveYaw(i) local mv = seq_move(self, i or self:GetSequence()); if mv:Length2D() <= 0 then return 0 end return math.deg(math.atan2(mv.y, mv.x)) end
function ENT:GetGroundSpeedVelocity() return self:GetSequenceVelocity(self:GetSequence()) * self:GetPlaybackRate() end
function ENT:FrameAdvance(dt) qwrite("ent.frame_advance", self.__id, tonumber(dt) or FrameTime()) end
function ENT:GetAnimTimeInterval() return FrameTime() end
function ENT:SetAnimation(act) local s = self:SelectWeightedSequence(act); if s >= 0 then self:ResetSequence(s) end end
function ENT:GetBoneContents(i) local m = minfo(self); local b = m and m.bone_props and m.bone_props[(i or 0) + 1]; return b and b.contents or 0 end
function ENT:GetBoneSurfaceProp(i) local m = minfo(self); local b = m and m.bone_props and m.bone_props[(i or 0) + 1]; return b and b.surface or "" end
function ENT:GetSequenceInfo(i) local m = minfo(self); i = i or self:GetSequence(); local name = m and m.sequences[i + 1]; if not name then return nil end local a = m.sequence_anim[i + 1] or { fps = 0, frames = 0, flags = 0 }; return { label = name, activityname = "", activity = -1, flags = a.flags, numframes = a.frames, fps = a.fps, fadeintime = 0.2, fadeouttime = 0.2, numevents = 0, numblends = 1, anims = { [0] = i } } end
function ENT:GetAnimCount() local m = minfo(self); return m and #m.anims or 0 end
function ENT:GetAnimInfo(i) local m = minfo(self); local a = m and m.anims[(i or 0) + 1]; if not a then return nil end return { label = a.name, fps = a.fps, numframes = a.frames, flags = a.flags } end
function ENT:GetCollisionBounds()
    local sn = snap(self.__id)
    if sn and sn.mins and sn.maxs and (sn.maxs.x > sn.mins.x or sn.maxs.z > sn.mins.z) then
        return Vector(sn.mins.x, sn.mins.y, sn.mins.z), Vector(sn.maxs.x, sn.maxs.y, sn.maxs.z)
    end
    return self:GetModelBounds()
end
function ENT:SetCollisionBounds(mins, maxs) qwrite("ent.bounds", self.__id, { mins = { x = mins.x, y = mins.y, z = mins.z }, maxs = { x = maxs.x, y = maxs.y, z = maxs.z } }) end
function ENT:SetCollisionBoundsWS(mins, maxs) local p = self:GetPos(); self:SetCollisionBounds(mins - p, maxs - p) end
function ENT:GetSurroundingBounds() return self:WorldSpaceAABB() end
function ENT:OBBMins() local mn = self:GetCollisionBounds(); return mn end
function ENT:OBBMaxs() local _, mx = self:GetCollisionBounds(); return mx end
function ENT:OBBCenter() local mn, mx = self:GetCollisionBounds(); return (mn + mx) / 2 end
function ENT:BoundingRadius() return self:GetModelRadius() end
-- the network layer skips this entity for that player (Entity:SetPreventTransmit)
function ENT:SetPreventTransmit(ply, stop) if ply and ply.__pid then qwrite("ent.prevent_transmit", self.__id, { player = ply.__pid, prevent = stop ~= false }) end end
-- a parented child is sent to a client when its parent is (the network interest test follows the parent)
function ENT:SetTransmitWithParent(on) qwrite("ent.transmit_with_parent", self.__id, on ~= false) end
function ENT:GetTransmitWithParent() local s = snap(self.__id); return s ~= nil and s.transmit_with_parent == true end
-- the game's variants of SetModel: by name without precache / the world model of a weapon entity
function ENT:SetModelName(m) self:SetModel(m) end
function ENT:SetWeaponModel(m, wep) self:SetModel(m) end
function ENT:GetPreventTransmit(ply) local s = snap(self.__id); if not s or not s.transmit_block or not ply then return false end for _, p in ipairs(s.transmit_block) do if p == ply.__pid then return true end end return false end
-- touch callbacks on/off (on by default here; the game needs SetTrigger(true)) and the padded touch box
function ENT:SetTrigger(on) qwrite("ent.trigger", self.__id, on == true) end
function ENT:UseTriggerBounds(on, pad) qwrite("ent.trigger_bounds", self.__id, { on = on ~= false, pad = tonumber(pad) or 0 }) end
function ENT:GetModelScale() local s = snap(self.__id); local v = s and s.model_scale; if v == nil or v <= 0 then return 1 end return v end
-- render scale only (physics untouched, as in the game); the deltaTime blend is not kept
function ENT:SetModelScale(scale, deltaTime) qwrite("ent.model_scale", self.__id, tonumber(scale) or 1) end
function ENT:GetFriction() local sn = snap(self.__id); if sn and sn.friction then return sn.friction end local m = minfo(self); return m and m.friction or 0 end
function ENT:GetElasticity() local sn = snap(self.__id); if sn and sn.elasticity then return sn.elasticity end local m = minfo(self); return m and m.elasticity or 0 end
function ENT:SetFriction(f) qwrite("set_surface", self.__id, { friction = f }) end
function ENT:IsOnFire() local sn = snap(self.__id); return sn ~= nil and sn.on_fire == true end
function ENT:ForcePlayerDrop() qwrite("force_drop", self.__id, nil) end
function ENT:SetBodyGroups(str)
    for i = 1, #tostring(str) do
        local v = tonumber(string.sub(str, i, i)) or 0
        self:SetBodygroup(i - 1, v)
    end
end
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
function ENT:WorldSpaceAABB() local mn, mx = self:GetCollisionBounds(); local lo, hi = self:GetRotatedAABB(mn, mx); local p = self:GetPos(); return lo + p, hi + p end
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
function ENT:GetFlexBounds(i) local m = minfo(self); local f = m and m.flex_info and m.flex_info[(i or 0) + 1]; if not f then return 0, 0 end return f.min, f.max end
function ENT:GetFlexType(i) local m = minfo(self); local f = m and m.flex_info and m.flex_info[(i or 0) + 1]; return f and f.kind or "" end
function ENT:GetMaterials() local m = minfo(self); local out = {}; for _, n in ipairs(m and m.materials or {}) do out[#out + 1] = n end return out end
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
-- @hook ENTITY:AcceptInput @hook WEAPON:AcceptInput (a scripted weapon instance takes the input the same way)
function ENT:Input(name, activator, caller, data) if hook.Run("AcceptInput", self, name, activator, caller, data) == true then return end local si = scripted_ents and scripted_ents.__instance and scripted_ents.__instance(self.__id); if si and si.AcceptInput and si:AcceptInput(name, activator or NULL, caller or NULL, data) == true then return true end if self.__id >= 0x20000000 and self.__id < 0x40000000 then qwrite("fire", self.__id, { input = name, param = data == nil and "" or tostring(data), activator = (type(activator) == "table" and rawget(activator, "__id")) or 0 }) return true end if name == "Kill" then self:Remove() return true end return false end
function ENT:Fire(input, param, delay, activator, caller)
    if (delay or 0) > 0 then local me = self; timer.Simple(delay, function() if IsValid(me) then me:Input(input, activator, caller, param) end end) else self:Input(input, activator, caller, param) end
end
-- Lua-side entity outputs as base_entity keeps them: "targets,input,param,delay,times" per
-- output name; TriggerOutput fires every stored entry through Entity:Fire (times counts down,
-- -1 = forever), !self / !activator resolve, other targets by name
-- @hook ENTITY:StoreOutput
function ENT:StoreOutput(name, info)
    local outs = rawget(self, "__outputs") or {}
    rawset(self, "__outputs", outs)
    outs[name] = outs[name] or {}
    local f = {}
    for part in string.gmatch(tostring(info) .. ",", "([^,]*),") do f[#f + 1] = part end
    table.insert(outs[name], { entities = f[1] or "", input = f[2] or "", param = f[3] or "", delay = tonumber(f[4]) or 0, times = tonumber(f[5]) or -1 })
end
-- @hook ENTITY:TriggerOutput
function ENT:TriggerOutput(name, activator, data)
    local outs = rawget(self, "__outputs")
    local list = outs and outs[name]
    if not list then return end
    for i = #list, 1, -1 do
        local o = list[i]
        local targets
        if o.entities == "!self" then targets = { self }
        elseif o.entities == "!activator" then targets = { activator }
        else targets = (ents.FindByName and ents.FindByName(o.entities)) or {} end
        -- info_node_hint nodes and func_tank guns are not entities here: their inputs go to the server by name
        if o.input == "EnableHint" or o.input == "DisableHint" or o.input == "Activate" or o.input == "Deactivate" then qwrite("ai_hint_input", self.__id, { name = o.entities, input = o.input }) end
        for _, t in ipairs(targets) do
            if IsValid(t) then t:Fire(o.input, o.param ~= "" and o.param or data, o.delay, activator, self) end
        end
        if o.times > 0 then
            o.times = o.times - 1
            if o.times == 0 then table.remove(list, i) end
        end
    end
end
-- @hook ENTITY:AddOutputFromKeyValue
function ENT:AddOutputFromKeyValue(k, v) if string.sub(tostring(k), 1, 2) == "On" then self:StoreOutput(k, v) return true end return false end
-- @hook ENTITY:AddOutputFromAcceptInput
function ENT:AddOutputFromAcceptInput(name, data)
    if name ~= "AddOutput" then return false end
    local k, v = string.match(tostring(data), "^%s*(%S+)%s+(.-)%s*$")
    if k then return self:AddOutputFromKeyValue(k, v) end
    return false
end
function ENT:ClearAllOutputs() rawset(self, "__outputs", {}) end
function ENT:EmitStepSound() end
FL_ONGROUND, FL_DUCKING, FL_ANIMDUCKING, FL_WATERJUMP, FL_ONTRAIN, FL_INRAIN, FL_FROZEN, FL_ATCONTROLS, FL_CLIENT, FL_FAKECLIENT, FL_INWATER, FL_FLY, FL_SWIM, FL_CONVEYOR, FL_NPC, FL_GODMODE, FL_NOTARGET, FL_AIMTARGET, FL_PARTIALGROUND, FL_STATICPROP, FL_GRAPHED, FL_GRENADE, FL_STEPMOVEMENT, FL_DONTTOUCH, FL_BASEVELOCITY, FL_WORLDBRUSH, FL_OBJECT, FL_KILLME, FL_ONFIRE, FL_DISSOLVING, FL_TRANSRAGDOLL, FL_UNBLOCKABLE_BY_PLAYER = 1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608, 16777216, 33554432, 67108864, 134217728, 268435456, 536870912, 1073741824, 2147483648
MOVETYPE_NONE, MOVETYPE_ISOMETRIC, MOVETYPE_WALK, MOVETYPE_STEP, MOVETYPE_FLY, MOVETYPE_FLYGRAVITY, MOVETYPE_VPHYSICS, MOVETYPE_PUSH, MOVETYPE_NOCLIP, MOVETYPE_LADDER, MOVETYPE_OBSERVER, MOVETYPE_CUSTOM = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11
MOVECOLLIDE_DEFAULT, MOVECOLLIDE_FLY_BOUNCE, MOVECOLLIDE_FLY_CUSTOM, MOVECOLLIDE_FLY_SLIDE = 0, 1, 2, 3
SOLID_NONE, SOLID_BSP, SOLID_BBOX, SOLID_OBB, SOLID_OBB_YAW, SOLID_CUSTOM, SOLID_VPHYSICS = 0, 1, 2, 3, 4, 5, 6
COLLISION_GROUP_NONE, COLLISION_GROUP_DEBRIS, COLLISION_GROUP_DEBRIS_TRIGGER, COLLISION_GROUP_INTERACTIVE_DEBRIS, COLLISION_GROUP_INTERACTIVE, COLLISION_GROUP_PLAYER, COLLISION_GROUP_BREAKABLE_GLASS, COLLISION_GROUP_VEHICLE, COLLISION_GROUP_PLAYER_MOVEMENT, COLLISION_GROUP_NPC, COLLISION_GROUP_IN_VEHICLE, COLLISION_GROUP_WEAPON, COLLISION_GROUP_VEHICLE_CLIP, COLLISION_GROUP_PROJECTILE, COLLISION_GROUP_DOOR_BLOCKER, COLLISION_GROUP_PASSABLE_DOOR, COLLISION_GROUP_DISSOLVING, COLLISION_GROUP_PUSHAWAY, COLLISION_GROUP_NPC_ACTOR, COLLISION_GROUP_NPC_SCRIPTED, COLLISION_GROUP_WORLD = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20
BLOOD_COLOR_RED, BLOOD_COLOR_YELLOW, BLOOD_COLOR_GREEN, BLOOD_COLOR_MECH, BLOOD_COLOR_ANTLION, BLOOD_COLOR_ZOMBIE, BLOOD_COLOR_ANTLION_WORKER, DONT_BLEED = 0, 1, 2, 3, 4, 5, 6, -1
MAT_DEFAULT = 67
NPC_STATE_INVALID, NPC_STATE_NONE, NPC_STATE_IDLE, NPC_STATE_ALERT, NPC_STATE_COMBAT, NPC_STATE_SCRIPT, NPC_STATE_PLAYDEAD, NPC_STATE_PRONE, NPC_STATE_DEAD = -1, 0, 1, 2, 3, 4, 5, 6, 7
-- model-data, parenting, render and map-IO methods: honest stubs until the engine side exists
-- the server animator: sequence, cycle, rate, finished flag and one gesture layer (id 0)
local function seqname(self, i) if type(i) == "number" then return self:GetSequenceName(i) end return tostring(i or "") end
local function gest(self) local sn = snap(self.__id); return sn and sn.gesture end
local function setlayer(self, id, t) if id == 0 then qwrite("ent.gesture", self.__id, t) end end
function ENT:GetCycle() local sn = snap(self.__id); return sn and sn.cycle or 0 end
function ENT:SetCycle(c) qwrite("ent.cycle", self.__id, tonumber(c) or 0) end
function ENT:GetPlaybackRate() local sn = snap(self.__id); local r = sn and sn.playback_rate; if r == nil then return 1 end return r end
function ENT:SetPlaybackRate(r) qwrite("ent.rate", self.__id, tonumber(r) or 1) end
function ENT:SetSequence(i) qwrite("ent.sequence", self.__id, seqname(self, i)) end
function ENT:ResetSequence(i) qwrite("ent.sequence", self.__id, seqname(self, i)) end
function ENT:ResetSequenceInfo() qwrite("ent.reset_sequence", self.__id, nil) end
function ENT:IsSequenceFinished() local sn = snap(self.__id); return sn ~= nil and sn.sequence_finished == true end
function ENT:AddGestureSequence(i, autokill) qwrite("ent.gesture", self.__id, { op = "add", sequence = seqname(self, i), autokill = autokill ~= false }); return 0 end
function ENT:AddLayeredSequence(i, priority) qwrite("ent.gesture", self.__id, { op = "add", sequence = seqname(self, i), autokill = true }); if priority then setlayer(self, 0, { priority = math.floor(tonumber(priority) or 0) }) end return 0 end
function ENT:FindGestureSequenceLayer(i) local g = gest(self); if g and g.sequence == seqname(self, i) then return 0 end return -1 end
function ENT:RemoveAllGestures() qwrite("ent.gesture", self.__id, { op = "remove" }) end
function ENT:RemoveLayer(id) if id == 0 then qwrite("ent.gesture", self.__id, { op = "remove" }) end end
function ENT:GetLayerCycle(id) local g = gest(self); if id ~= 0 or not g then return 0 end return g.cycle end
function ENT:GetLayerPlaybackRate(id) local g = gest(self); if id ~= 0 or not g then return 0 end return g.rate end
function ENT:GetLayerWeight(id) local g = gest(self); if id ~= 0 or not g then return 0 end return g.weight end
function ENT:GetLayerSequence(id) local g = gest(self); if id ~= 0 or not g then return -1 end return self:LookupSequence(g.sequence) end
function ENT:GetLayerDuration(id) local g = gest(self); if id ~= 0 or not g or (g.rate or 0) == 0 then return 0 end return self:SequenceDuration(self:LookupSequence(g.sequence)) / g.rate end
function ENT:SetLayerCycle(id, c) setlayer(self, id, { cycle = tonumber(c) or 0 }) end
function ENT:SetLayerPlaybackRate(id, r) setlayer(self, id, { rate = tonumber(r) or 1 }) end
function ENT:SetLayerWeight(id, w) setlayer(self, id, { weight = tonumber(w) or 1 }) end
function ENT:SetLayerLooping(id, l) setlayer(self, id, { looping = l == true }) end
function ENT:SetLayerAutokill(id, k) setlayer(self, id, { autokill = k == true }) end
function ENT:SetLayerPriority(id, p) setlayer(self, id, { priority = math.floor(tonumber(p) or 0) }) end
function ENT:SetLayerSequence(id, i) setlayer(self, id, { sequence = seqname(self, i) }) end
function ENT:SetLayerDuration(id, d) local g = gest(self); d = tonumber(d) or 0; if id ~= 0 or not g or d <= 0 then return end local base = self:SequenceDuration(self:LookupSequence(g.sequence)); if base > 0 then setlayer(self, id, { rate = base / d }) end end

-- pose parameters: the model's table (name, range, loop) and the values the server holds
local function poseparams(self) local m = minfo(self); return m and m.pose_params or {} end
function ENT:GetNumPoseParameters() return #poseparams(self) end
function ENT:GetPoseParameterName(i) local p = poseparams(self)[(i or 0) + 1]; return p and p.name or "" end
function ENT:LookupPoseParameter(name) local n = string.lower(tostring(name)); for i, p in ipairs(poseparams(self)) do if string.lower(p.name) == n then return i - 1 end end return -1 end
function ENT:GetPoseParameterRange(i) local p = poseparams(self)[(i or 0) + 1]; if not p then return 0, 0 end return p.start, p["end"] end
function ENT:GetPoseParameter(name) local sn = snap(self.__id); local pose = sn and sn.pose; if not pose then return 0 end local n = string.lower(tostring(name)); for k, v in pairs(pose) do if string.lower(k) == n then return v end end return 0 end
function ENT:SetPoseParameter(name, v) if type(name) == "number" then name = self:GetPoseParameterName(name) end qwrite("ent.pose", self.__id, { name = tostring(name), value = tonumber(v) or 0 }) end
function ENT:ClearPoseParameters() qwrite("ent.pose_clear", self.__id, nil) end

-- activities: the ACT_* name each sequence carries in the MDL, resolved through the ACT enum
local ACT_NAMES = {}
for k, v in pairs(_G) do if type(k) == "string" and k:sub(1, 4) == "ACT_" and type(v) == "number" then ACT_NAMES[v] = k end end
local function actid(act) if type(act) == "number" then return act end local v = _G[tostring(act)]; return type(v) == "number" and v or -1 end
local function seqacts(self) local m = minfo(self); return m and m.sequence_activity or {} end
function ENT:GetSequenceActivityName(i) local a = seqacts(self)[(i or 0) + 1]; if not a or a.name == "" then return "Not Found!" end return a.name end
function ENT:GetSequenceActivity(i) local a = seqacts(self)[(i or 0) + 1]; if not a or a.name == "" then return -1 end return actid(a.name) end
local function weighted(self, act, pick)
    act = actid(act)
    local total, cands = 0, {}
    for i, a in ipairs(seqacts(self)) do
        if a.name ~= "" and actid(a.name) == act then local w = math.max(a.weight or 0, 1); total = total + w; cands[#cands + 1] = { i - 1, w } end
    end
    if total == 0 then return -1 end
    local r = pick(total)
    for _, c in ipairs(cands) do r = r - c[2]; if r <= 0 then return c[1] end end
    return cands[#cands][1]
end
function ENT:SelectWeightedSequence(act) return weighted(self, act, function(total) return math.random(1, total) end) end
function ENT:SelectWeightedSequenceSeeded(act, seed) return weighted(self, act, function(total) return (math.floor(tonumber(seed) or 0) % total) + 1 end) end
local function layeract(self) local g = gest(self); if not g then return nil end local i = self:LookupSequence(g.sequence); if i < 0 then return nil end return self:GetSequenceActivity(i) end
function ENT:AddGesture(act, autokill) local i = self:SelectWeightedSequence(act); if i < 0 then return -1 end return self:AddGestureSequence(i, autokill) end
function ENT:FindGestureLayer(act) if layeract(self) == actid(act) then return 0 end return -1 end
function ENT:IsPlayingGesture(act) return layeract(self) == actid(act) end
function ENT:RemoveGesture(act) if layeract(self) == actid(act) then self:RemoveAllGestures() end end
function ENT:RestartGesture(act, add, autokill)
    if layeract(self) == actid(act) then setlayer(self, 0, { cycle = 0 }) return 0 end
    if add ~= false then return self:AddGesture(act, autokill) end
    return -1
end

for _, name in ipairs({ "BoundingRadius", "FindBodygroupByName", "ForcePlayerDrop", "GetAnimCount", "GetAnimInfo", "GetAttachment", "GetAttachments", "GetBodyGroups", "GetBodygroupCount", "GetBodygroupName", "GetBoneCount", "GetBoneName", "GetBoneParent", "GetColor", "GetColor4Part", "GetElasticity", "GetFlexIDByName", "GetFlexName", "GetFlexNum", "GetFlexScale", "GetFlexWeight", "GetFriction", "GetHitBoxCount", "GetHitboxSetCount", "GetManipulateBoneAngles", "GetManipulateBoneScale", "GetMaterial", "GetModelBounds", "GetModelRadius", "GetModelRenderBounds", "GetModelScale", "GetNoDraw", "GetNumBodyGroups", "GetParentWorldTransformMatrix", "GetRenderFX", "GetRenderMode", "GetRotatedAABB", "GetSequence", "GetSequenceCount", "GetSequenceInfo", "GetSequenceList", "GetSequenceName", "GetWorldTransformMatrix", "HasBoneManipulations", "HasFlexManipulatior", "IsOnFire", "IsPointInBounds", "LookupAttachment", "LookupBone", "LookupSequence", "ManipulateBoneAngles", "ManipulateBoneScale", "NearestPoint", "OBBMaxs", "OBBMins", "SequenceDuration", "SetBodyGroups", "SetColor", "SetColor4Part", "SetElasticity", "SetEyeTarget", "SetFlexScale", "SetFlexWeight", "SetFriction", "SetMaterial", "SetMoveParent", "SetNoDraw", "SetParent", "SetRenderFX", "SetRenderMode", "WorldSpaceAABB" }) do
    if ENT[name] == nil then ENT[name] = stub("Entity:" .. name) end
end

------------------------------------------------------------------------ NPC: the rest
-- Our AI has no schedules / conditions / relationships (not ported by policy); what it has is a
-- target, a chase, a path goal and a squad, carried by the snapshot.
function NPC:GetEnemy() local s = snap(self.__id); if s and s.npc_target and s.npc_target ~= 0 then return Player(s.npc_target) end return NULL end
function NPC:CapabilitiesGet() local s = snap(self.__id); return s and s.npc_caps or 0 end
function NPC:CapabilitiesHas(c) c = c or 0; return c ~= 0 and bit.band(self:CapabilitiesGet(), c) == c end
function NPC:GetTimeEnemyLastReacquired() local s = snap(self.__id); local t = s and s.npc_target_first_seen or 0; if t == 0 then return 0 end return t * engine.TickInterval() end
function NPC:HasObstacles() local s = snap(self.__id); return (s and s.npc_stuck or 0) > 0 end
function NPC:GetArrivalDistance() local s = snap(self.__id); return s and s.npc_chase_reach or 0 end
function NPC:SetArrivalDistance(d) qwrite("npc_arrival", self.__id, { dist = tonumber(d) or 0 }) end
-- a fixed think cadence instead of the measured near / far one (0 restores it)
function NPC:SetMoveInterval(s) qwrite("npc_move_interval", self.__id, { seconds = tonumber(s) or 0 }) end
-- fire from a crouch (true) / never (false) / as the combat state says (nil)
function NPC:SetForceCrouch(b) if b == nil then qwrite("npc_force_crouch", self.__id, {}) else qwrite("npc_force_crouch", self.__id, { on = b == true }) end end
function NPC:MovePause() qwrite("npc_move_pause", self.__id, { paused = true }) end
function NPC:MoveStart() qwrite("npc_move_pause", self.__id, { paused = false }) end
-- ENTITY:SetMaxYawSpeed of scripted NPCs: turn-rate override (deg/s) on the server AI; 0 restores
-- the measured class rate (150 deg/s + 0.267 per u/s while halting toward a new goal)
-- @hook ENTITY:SetMaxYawSpeed
function ENT:SetMaxYawSpeed(r) qwrite("npc_max_yaw", self.__id, { rate = tonumber(r) or 0 }) end
function NPC:GetIdealMoveSpeed() local s = snap(self.__id); return s and s.npc_move_speed or 0 end
function NPC:GetMoveInterval() return FrameTime() end
function NPC:GetIdealYaw()
    local s = snap(self.__id); local me = self:GetPos(); local to
    local e = self:GetEnemy(); if e and e:IsValid() then to = e:GetPos() elseif s and s.npc_next_goal then to = Vector(s.npc_next_goal.x, s.npc_next_goal.y, s.npc_next_goal.z) end
    if not to then return self:GetAngles().y end
    return math.deg(math.atan2(to.y - me.y, to.x - me.x))
end
function NPC:IsFacingIdealYaw() local d = (self:GetIdealYaw() - self:GetAngles().y + 180) % 360 - 180; return math.abs(d) <= 15 end
function NPC:MarkTookDamageFromEnemy() qwrite("npc.took_damage", self.__id, nil) end
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
-- the eye offset the server's sight / aim use (the measured 64 u unless a script set one)
function NPC:GetViewOffset() local s = snap(self.__id); local v = s and s.npc_view_offset; if v then return Vector(v.x, v.y, v.z) end return Vector(0, 0, 64) end
function NPC:SetViewOffset(v) if v == nil then qwrite("npc_view_offset", self.__id, {}) else qwrite("npc_view_offset", self.__id, { x = v.x, y = v.y, z = v.z }) end end
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
function NPC:GetSquad() local s = snap(self.__id); if s and s.npc_ai_squad then return s.npc_ai_squad end return rawget(self, "__squad") end
function NPC:SetSquad(name) rawset(self, "__squad", name); qwrite("npc_ai_squad", self.__id, { name = name }) end
function NPC:GetHintGroup() return rawget(self, "__hint_group") or "" end
function NPC:SetHintGroup(name) rawset(self, "__hint_group", name); qwrite("npc_ai_hint_group", self.__id, { name = name }) end
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
-- disposition toward players: the scripted one (AddRelationship / AddEntityRelationship) when
-- set, else from the class; other entities are not targets of our AI (the class answer)
local function class_of(e)
    if e == nil then return CLASS_NONE end
    if e.IsPlayer and e:IsPlayer() then return CLASS_PLAYER end
    if e.IsNPC and e:IsNPC() and e.Classify then return e:Classify() end
    return CLASS_NONE
end
local function default_disposition(a, b)
    if a == b and a ~= CLASS_NONE then return D_LI end
    local human = function(c) return c == CLASS_PLAYER or c == CLASS_PLAYER_ALLY or c == CLASS_PLAYER_ALLY_VITAL or c == CLASS_VORTIGAUNT end
    local combine = function(c) return c == CLASS_COMBINE or c == CLASS_METROPOLICE end
    local monster = function(c) return c == CLASS_ZOMBIE or c == CLASS_HEADCRAB or c == CLASS_ANTLION or c == CLASS_BARNACLE end
    if a == CLASS_NONE or a == CLASS_BULLSEYE then return D_NU end
    if a == CLASS_EARTH_FAUNA then return D_FR end
    if human(a) then if human(b) then return D_LI elseif combine(b) or monster(b) then return D_HT end return D_NU end
    if combine(a) then if combine(b) then return D_LI elseif human(b) or monster(b) then return D_HT end return D_NU end
    if a == CLASS_ZOMBIE or a == CLASS_HEADCRAB then if b == CLASS_ZOMBIE or b == CLASS_HEADCRAB then return D_LI elseif human(b) or combine(b) or b == CLASS_ANTLION then return D_HT end return D_NU end
    if a == CLASS_ANTLION then if human(b) or combine(b) or b == CLASS_ZOMBIE or b == CLASS_HEADCRAB then return D_HT end return D_NU end
    if a == CLASS_BARNACLE then if b == CLASS_BARNACLE then return D_LI end return D_HT end
    return D_NU
end
-- per-entity row, then the class row ("player", an npc_* class), then the game's class table
function NPC:Disposition(e)
    if e == nil then return D_ER end
    local er = rawget(self, "__ent_rel"); if er and er[e.__id] then return er[e.__id] end
    local cr = rawget(self, "__class_rel")
    if cr then
        local key = (e.IsPlayer and e:IsPlayer()) and "player" or e:GetClass()
        if cr[key] then return cr[key] end
    end
    local s = snap(self.__id)
    if s and s.npc_relation and e.IsPlayer and e:IsPlayer() then return s.npc_relation end
    local own = rawget(self, "__npc_class") or self:Classify()
    return default_disposition(own, class_of(e))
end
local function disp_code(d)
    if type(d) == "number" then return math.floor(d) end
    if type(d) == "string" then local g = _G[d]; if type(g) == "number" then return g end return tonumber(d) end
    return nil
end
-- "player D_HT 99" / AddEntityRelationship: the server keeps per-entity and per-class rows over
-- the game's class table (hl2 relationship matrix); a player row also drives who the body hunts
function NPC:AddRelationship(str)
    local target, disp, prio = tostring(str):match("^%s*(%S+)%s+(%S+)%s*(%S*)")
    if not target then return end
    local d = disp_code(disp); if d == nil then return end
    local rows = rawget(self, "__class_rel") or {}; rows[target] = d; rawset(self, "__class_rel", rows)
    qwrite("npc_ai_relationship", self.__id, { class = target, disp = d, prio = tonumber(prio) or 0 })
end
function NPC:AddEntityRelationship(e, disp, prio)
    if not IsValid(e) then return end
    local d = disp_code(disp); if d == nil then return end
    if e.IsPlayer and e:IsPlayer() then
        -- players are keyed as the "player" class row (the body's hostility flag follows it)
        local rows = rawget(self, "__class_rel") or {}; rows.player = d; rawset(self, "__class_rel", rows)
        qwrite("npc_ai_relationship", self.__id, { class = "player", disp = d, prio = tonumber(prio) or 0 })
        return
    end
    local rows = rawget(self, "__ent_rel") or {}; rows[e.__id] = d; rawset(self, "__ent_rel", rows)
    qwrite("npc_ai_relationship", self.__id, { other = e.__id, disp = d, prio = tonumber(prio) or 0 })
end
function NPC:GetWeapon() local w = rawget(self, "__weapon"); if w then return w end return NULL end
function NPC:GetWeapons() local w = self:GetWeapon(); if IsValid(w) then return { w } end return {} end
function NPC:GetActivity() return self:GetVelocity():Length() > 1 and ACT_RUN or ACT_IDLE end
function NPC:IsCrouching() return false end
function NPC:GetFOV() return 360 end
HULL_HUMAN, HULL_SMALL_CENTERED, HULL_WIDE_HUMAN, HULL_TINY, HULL_WIDE_SHORT, HULL_MEDIUM, HULL_TINY_CENTERED, HULL_LARGE, HULL_LARGE_CENTERED, HULL_MEDIUM_TALL = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9
NAV_GROUND, NAV_JUMP, NAV_FLY, NAV_CLIMB = 0, 1, 2, 3
CLASS_NONE, CLASS_PLAYER, CLASS_PLAYER_ALLY, CLASS_PLAYER_ALLY_VITAL, CLASS_ANTLION, CLASS_BARNACLE, CLASS_BULLSEYE, CLASS_CITIZEN_PASSIVE, CLASS_CITIZEN_REBEL, CLASS_COMBINE, CLASS_COMBINE_GUNSHIP, CLASS_CONSCRIPT, CLASS_HEADCRAB, CLASS_MANHACK, CLASS_METROPOLICE, CLASS_MILITARY, CLASS_SCANNER, CLASS_STALKER, CLASS_VORTIGAUNT, CLASS_ZOMBIE, CLASS_PROTOSNIPER, CLASS_MISSILE, CLASS_FLARE, CLASS_EARTH_FAUNA, CLASS_HACKED_ROLLERMINE, CLASS_COMBINE_HUNTER = 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25
D_ER, D_HT, D_FR, D_LI, D_NU = 0, 1, 2, 3, 4
-- an ideal yaw set by script (an idle NPC turns to it at its yaw rate); otherwise the derived one
local derived_ideal_yaw = NPC.GetIdealYaw
function NPC:GetIdealYaw() local s = snap(self.__id); if s and s.npc_ideal_yaw ~= nil then return s.npc_ideal_yaw end return derived_ideal_yaw(self) end
function NPC:SetIdealYaw(y) qwrite("npc_ideal_yaw", self.__id, { yaw = tonumber(y) or 0 }) end
function NPC:UpdateYaw() qwrite("npc_ideal_yaw", self.__id, { update = true }) end
function NPC:SetIdealYawAndUpdate(y) qwrite("npc_ideal_yaw", self.__id, { yaw = tonumber(y) or 0, update = true }) end
function NPC:SetMoveYawLocked(b) qwrite("npc_move_yaw_locked", self.__id, { on = b ~= false }) end
function NPC:IsMoveYawLocked() local s = snap(self.__id); return s ~= nil and s.npc_move_yaw_locked == true end


------------------------------------------------------------------------ PhysObj: the rest
local PH = __glua_classes.PhysObj
local function pminfo(self) local e = self.__ent; return e and minfo(e) or nil end
local function mesh_verts(list, out)
    for i = 1, #list - 2, 3 do
        local a, b, c = list[i], list[i + 1], list[i + 2]
        local va, vb, vc = Vector(a.x, a.y, a.z), Vector(b.x, b.y, b.z), Vector(c.x, c.y, c.z)
        local n = (vb - va):Cross(vc - va); n:Normalize()
        for _, v in ipairs({ va, vb, vc }) do out[#out + 1] = { pos = v, normal = n, u = 0, v = 0 } end
    end
    return out
end
function PH:GetAABB() local m = pminfo(self); if not m or m.phys_mins == nil then return Vector(), Vector() end return Vector(m.phys_mins.x, m.phys_mins.y, m.phys_mins.z), Vector(m.phys_maxs.x, m.phys_maxs.y, m.phys_maxs.z) end
function PH:GetVolume() local m = pminfo(self); return m and m.volume or 0 end
function PH:GetSurfaceArea() local m = pminfo(self); return m and m.surface_area or 0 end
function PH:GetMassCenter() local m = pminfo(self); if not m or m.mass_center == nil then return Vector() end return Vector(m.mass_center.x, m.mass_center.y, m.mass_center.z) end
-- rotation inertia (kg u^2, body axes): the live body's (the PHY solid's per-unit inertia times the
-- mass, or what SetInertia set), else from the model data
function PH:GetInertia()
    local s = psn(self); local i = s and s.inertia
    if i and (i.x > 0 or i.y > 0 or i.z > 0) then return Vector(i.x, i.y, i.z) end
    local m = pminfo(self); if not m or m.inertia == nil then return Vector() end local k = self:GetMass(); return Vector(m.inertia.x * k, m.inertia.y * k, m.inertia.z * k)
end
function PH:GetInvInertia() local i = self:GetInertia(); local function inv(x) return x > 0 and 1 / x or 0 end return Vector(inv(i.x), inv(i.y), inv(i.z)) end
function PH:GetMeshConvexes() local m = pminfo(self); local out = {}; if m and m.mesh then for i, c in ipairs(m.mesh) do out[i] = mesh_verts(c, {}) end end return out end
function PH:GetMesh() local m = pminfo(self); local out = {}; if m and m.mesh then for _, c in ipairs(m.mesh) do mesh_verts(c, out) end end return out end
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

------------------------------------------------------------------------ Player: the rest
-- Movement constants are the engine's measured ones (hull 32x32x72 / 36 ducked, eyes 64 / 28,
-- step 18); setters without a server op stay stubs rather than pretending.
function PLY:GetCurrentViewOffset() if self:Crouching() then return self:GetViewOffsetDucked() end return self:GetViewOffset() end
local function mv(self, f, d) local p = psnap(self.__pid); local v = p and p[f]; return v ~= nil and v or d end
local function mvset(self, which, v) local n = tonumber(v); qwrite("player.move_param", self.__pid, { x = which, y = n or 0, z = n ~= nil and 1 or 0 }) end
function PLY:WaterLevel() local p = psnap(self.__pid); return p and p.water_level or 0 end
function PLY:GetWalkSpeed() return mv(self, "walk_speed", 200) end
function PLY:GetRunSpeed() return mv(self, "run_speed", 400) end
function PLY:GetJumpPower() return mv(self, "jump_power", 200) end
function PLY:GetStepSize() return mv(self, "step_size", 18) end
function PLY:GetCrouchedWalkSpeed() return mv(self, "crouched_walk_speed", 0.3) end
function PLY:GetDuckSpeed() return mv(self, "duck_time", 0.4) end
function PLY:GetUnDuckSpeed() return mv(self, "unduck_time", 0.2) end
function PLY:GetViewOffset() return Vector(0, 0, mv(self, "stand_eye", 64)) end
function PLY:GetViewOffsetDucked() return Vector(0, 0, mv(self, "duck_eye", 28)) end
function PLY:GetHull() local h = mv(self, "hull_half", 16); return Vector(-h, -h, 0), Vector(h, h, mv(self, "stand_height", 72)) end
function PLY:GetHullDuck() local h = mv(self, "hull_half", 16); return Vector(-h, -h, 0), Vector(h, h, mv(self, "duck_height", 36)) end
function PLY:SetViewOffset(v) mvset(self, 8, v and v.z) end
function PLY:SetViewOffsetDucked(v) mvset(self, 9, v and v.z) end
function PLY:SetHull(mins, maxs) if maxs == nil then return end mvset(self, 10, maxs.x) mvset(self, 11, maxs.z) end
function PLY:SetHullDuck(mins, maxs) if maxs == nil then return end mvset(self, 10, maxs.x) mvset(self, 12, maxs.z) end
function PLY:ResetHull() mvset(self, 10, nil) mvset(self, 11, nil) mvset(self, 12, nil) end
function PLY:SetWalkSpeed(v) mvset(self, 0, v) end
function PLY:SetRunSpeed(v) mvset(self, 1, v) end
function PLY:SetJumpPower(v) mvset(self, 2, v) end
function PLY:SetStepSize(v) mvset(self, 3, v) end
function PLY:SetCrouchedWalkSpeed(v) mvset(self, 4, v) end
function PLY:SetDuckSpeed(v) mvset(self, 5, v) end
function PLY:SetUnDuckSpeed(v) mvset(self, 6, v) end
function PLY:SetMaxSpeed(v) mvset(self, 7, v) end
function PLY:GetMaxSpeed() if self:IsSprinting() then return self:GetRunSpeed() end return self:GetWalkSpeed() end
function PLY:GetSlowWalkSpeed() return mv(self, "slow_walk_speed", 100) end
function PLY:SetSlowWalkSpeed(v) mvset(self, 17, v) end
function PLY:GetLadderClimbSpeed() return mv(self, "ladder_speed", 200) end
function PLY:SetLadderClimbSpeed(v) mvset(self, 13, v) end
function PLY:GetLaggedMovementValue() return mv(self, "lagged_movement", 1) end
function PLY:SetLaggedMovementValue(v) mvset(self, 14, v) end
function PLY:SprintEnable() mvset(self, 15, 1) end
function PLY:SprintDisable() mvset(self, 15, 0) end
-- the sprint button counts as held in the mover until StopSprinting (+walk has no mover path: Start/StopWalking stay stubs)
function PLY:StartSprinting() mvset(self, 16, 1) end
function PLY:StopSprinting() mvset(self, 16, 0) end
function PLY:IsSprintEnabled() return mv(self, "sprint_enabled", true) end
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
function PLY:GetCurrentCommand()
    local p = psnap(self.__pid)
    local z = { x = 0, y = 0, z = 0 }
    local mv = p and p.cmd_move or z
    local va = p and p.view_angles or z
    return setmetatable({
        __ply = self,
        buttons = p and p.buttons or 0,
        move = { x = mv.x, y = mv.y, z = mv.z },
        view = { x = va.x, y = va.y, z = va.z },
        impulse = p and p.cmd_impulse or 0,
        mx = p and p.cmd_mouse_x or 0,
        my = p and p.cmd_mouse_y or 0,
        tick = p and p.cmd_tick or 0,
    }, __glua_classes.CUserCmd)
end
for _, name in ipairs({ "AllowFlashlight", "CanUseFlashlight", "DropNamedWeapon", "DropObject", "DropWeapon", "EnterVehicle", "ExitVehicle", "Flashlight", "FlashlightIsOn", "GetAmmo", "GetAmmoCount", "GetEntityInUse", "GetPlayerColor", "GetUseEntity", "GetWeaponColor", "GiveAmmo", "IsWalking", "KeyDownLast", "KeyPressed", "KeyReleased", "Kick", "LastHitGroup", "Lock", "UnLock", "RemoveAllAmmo", "RemoveAmmo", "Say", "SetAmmo", "SetDeaths", "SetFrags", "SetMaxArmor", "SetNoTarget", "SetPlayerColor", "SetSlowWalkSpeed", "SetWeaponColor", "StripAmmo", "TimeConnected" }) do
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
for _, name in ipairs({ "SetClip1", "SetClip2" }) do
    if WEP[name] == nil then WEP[name] = stub("Weapon:" .. name) end
end

------------------------------------------------------------------------ Vehicle: the rest
local veh_wheels = { prop_vehicle_jeep = 4, prop_vehicle_jeep_old = 4, prop_vehicle_apc = 4, prop_vehicle_airboat = 0, prop_vehicle_prisoner_pod = 0 }
local veh_boost = { prop_vehicle_jeep = true, prop_vehicle_jeep_old = true }
function VEH:GetSpeed() return self:GetVelocity():Length() end
function VEH:GetHLSpeed() return self:GetVelocity():Length() end
function VEH:GetPassenger(i) if (i or 0) == 0 then return self:GetDriver() end return NULL end
function VEH:IsValidVehicle() return self:IsValid() end
function VEH:HasBoost() return veh_boost[self:GetClass()] == true end
function VEH:IsBoosting() local v = vinfo(self); return v ~= nil and v.engine == true and (v.boost or 0) > 0 end
function VEH:SetBoost(b) qwrite("vehicle.boost", self.__id, tonumber(b) or 0) end
function VEH:BoostTimeLeft() return 0 end
function VEH:IsVehicleBodyInWater() return false end

function WEP:GetMaxClip1() local d = wdef(self); local n = d and d.Primary and d.Primary.ClipSize; return n or -1 end
function WEP:GetMaxClip2() local d = wdef(self); local n = d and d.Secondary and d.Secondary.ClipSize; return n or -1 end
function WEP:DefaultReload(_act)
    local owner = self:GetOwner()
    if owner == NULL then return false end
    local max, clip = self:GetMaxClip1(), self:Clip1()
    if max < 0 or clip >= max then return false end
    local d = wdef(self)
    local atype = d and d.Primary and d.Primary.Ammo or self:GetPrimaryAmmoType()
    local reserve = owner:GetAmmoCount(atype)
    local take = math.min(max - clip, reserve)
    if take <= 0 then return false end
    self:SetClip1(clip + take)
    owner:RemoveAmmo(take, atype)
    -- @hook WEAPON:OnReloaded (our reload refills at once, so this is the same moment)
    if self.OnReloaded then self:OnReloaded() end
    return true
end

local function hbox(self, i) local m = minfo(self); return m and m.hitbox_list and m.hitbox_list[(i or 0) + 1] end
function ENT:GetHitBoxBone(i, _set) local h = hbox(self, i); return h and h.bone or -1 end
function ENT:GetHitBoxHitGroup(i, _set) local h = hbox(self, i); return h and h.group or -1 end
function ENT:GetHitBoxBounds(i, _set)
    local h = hbox(self, i)
    if not h then return nil end
    return Vector(h.mins.x, h.mins.y, h.mins.z), Vector(h.maxs.x, h.maxs.y, h.maxs.z)
end
function ENT:GetHitBoxGroupCount() return self:GetHitboxSetCount() end
function ENT:GetHitboxSet() if self:GetHitboxSetCount() > 0 then return 0, "default" end return nil end
function ENT:GetChildBones(b)
    local m, out = minfo(self), {}
    if not m then return out end
    for i, p in ipairs(m.bone_parents) do if p == b then out[#out + 1] = i - 1 end end
    return out
end

function PLY:GetPreviousWeapon()
    local p = psnap(self.__pid)
    if p and p.previous_weapon ~= "" and self:HasWeapon(p.previous_weapon) then return WEP.__from(p.previous_weapon, self.__pid) end
    return NULL
end
EF_BONEMERGE, EF_BRIGHTLIGHT, EF_DIMLIGHT, EF_NOINTERP, EF_NOSHADOW, EF_NODRAW = 1, 2, 4, 8, 16, 32
EF_NORECEIVESHADOW, EF_BONEMERGE_FASTCULL, EF_ITEM_BLINK, EF_PARENT_ANIMATES, EF_FOLLOWBONE = 64, 128, 256, 512, 1024
EFL_KILLME, EFL_DORMANT, EFL_NOCLIP_ACTIVE, EFL_SETTING_UP_BONES, EFL_KEEP_ON_RECREATE_ENTITIES = 1, 2, 4, 8, 16
EFL_DIRTY_SHADOWUPDATE, EFL_NOTIFY, EFL_FORCE_CHECK_TRANSMIT, EFL_BOT_FROZEN, EFL_SERVER_ONLY = 32, 64, 128, 256, 512
EFL_NO_AUTO_EDICT_ATTACH, EFL_DIRTY_ABSTRANSFORM, EFL_DIRTY_ABSVELOCITY, EFL_DIRTY_ABSANGVELOCITY = 1024, 2048, 4096, 8192
EFL_DIRTY_SURROUNDING_COLLISION_BOUNDS, EFL_DIRTY_SPATIAL_PARTITION, EFL_HAS_PLAYER_CHILD = 16384, 32768, 65536
EFL_IN_SKYBOX, EFL_USE_PARTITION_WHEN_NOT_SOLID, EFL_TOUCHING_FLUID = 131072, 262144, 524288
EFL_IS_BEING_LIFTED_BY_BARNACLE, EFL_NO_ROTORWASH_PUSH, EFL_NO_THINK_FUNCTION = 1048576, 2097152, 4194304
EFL_NO_GAME_PHYSICS_SIMULATION, EFL_CHECK_UNTOUCH, EFL_DONTBLOCKLOS, EFL_DONTWALKON = 8388608, 16777216, 33554432, 67108864
EFL_NO_DISSOLVE, EFL_NO_MEGAPHYSCANNON_RAGDOLL, EFL_NO_WATER_VELOCITY_CHANGE = 134217728, 268435456, 536870912
EFL_NO_PHYSCANNON_INTERACTION, EFL_NO_DAMAGE_FORCES = 1073741824, -2147483648

local function bone_bind(m, i)
    local b = m.bone_bind and m.bone_bind[i + 1]
    if not b then return nil end
    local pos, ang = Vector(b.pos.x, b.pos.y, b.pos.z), Angle(b.ang.x, b.ang.y, b.ang.z)
    local p = m.bone_parents[i + 1]
    if p and p >= 0 and p ~= i then
        local pp, pa = bone_bind(m, p)
        if pp then pos, ang = LocalToWorld(pos, ang, pp, pa) end
    end
    return pos, ang
end

function ENT:GetBonePosition(i)
    local m = minfo(self)
    if not m then return nil end
    local lp, la = bone_bind(m, i or 0)
    if not lp then return nil end
    return LocalToWorld(lp, la, self:GetPos(), self:GetAngles())
end

function ENT:GetBoneMatrix(i)
    local pos, ang = self:GetBonePosition(i)
    if not pos then return nil end
    local mtx = Matrix()
    mtx:SetAngles(ang)
    mtx:SetTranslation(pos)
    return mtx
end

------------------------------------------------------------------------ bones, sub-materials, flags, drag, shadows and the smaller Entity / PhysObj / Vehicle / Weapon methods
local function evars(self) local v = rawget(self, "__vars"); if not v then v = {}; rawset(self, "__vars", v) end return v end
function ENT:ManipulateBonePosition(b, p) qwrite("set_bone_manip", self.__id, { bone = b, pos = v3(p) }) end
function ENT:GetManipulateBonePosition(b) local sn = snap(self.__id); local v = sn and sn.bone_positions and sn.bone_positions[b]; return v and Vector(v.x, v.y, v.z) or Vector(0, 0, 0) end
function ENT:ManipulateBoneJiggle(b, on) local v = evars(self); v.__jiggle = v.__jiggle or {}; v.__jiggle[b] = on end
function ENT:GetManipulateBoneJiggle(b) local j = evars(self).__jiggle; return j and j[b] or 0 end
local base_has_manip = ENT.HasBoneManipulations
function ENT:HasBoneManipulations() if base_has_manip(self) then return true end local sn = snap(self.__id); return sn ~= nil and sn.bone_positions ~= nil and next(sn.bone_positions) ~= nil end
function ENT:SetSubMaterial(i, m) if i == nil then qwrite("set_sub_material", self.__id, {}) else qwrite("set_sub_material", self.__id, { index = i, material = m or "" }) end end
function ENT:GetSubMaterial(i) local sn = snap(self.__id); return sn and sn.sub_materials and sn.sub_materials[i] or "" end
local base_flags = ENT.GetFlags
function ENT:GetFlags() local f = base_flags(self); local v = rawget(self, "__vars"); if v then f = bit.band(bit.bor(f, v.__flags_add or 0), bit.bnot(v.__flags_rem or 0)) end return f end
function ENT:AddFlags(f)
    local v = evars(self); v.__flags_add = bit.bor(v.__flags_add or 0, f); v.__flags_rem = bit.band(v.__flags_rem or 0, bit.bnot(f))
    if bit.band(f, FL_FROZEN) ~= 0 then local p = self:GetPhysicsObject(); if IsValid(p) then p:EnableMotion(false) end end
end
function ENT:RemoveFlags(f)
    local v = evars(self); v.__flags_rem = bit.bor(v.__flags_rem or 0, f); v.__flags_add = bit.band(v.__flags_add or 0, bit.bnot(f))
    if bit.band(f, FL_FROZEN) ~= 0 then local p = self:GetPhysicsObject(); if IsValid(p) then p:EnableMotion(true) end end
end
-- plain per-entity settings the engine only stores
local function setter(name, key) ENT["Set" .. name] = function(self, val) evars(self)[key] = val end end
local function getter(name, key, default) ENT["Get" .. name] = function(self) local v = evars(self)[key]; if v == nil then return default end return v end end
setter("SpawnEffect", "__spawn_effect"); getter("SpawnEffect", "__spawn_effect", false)
setter("ShouldPlayPickupSound", "__pickup_sound"); getter("ShouldPlayPickupSound", "__pickup_sound", true)
setter("ShouldServerRagdoll", "__server_ragdoll"); getter("ShouldServerRagdoll", "__server_ragdoll", false)
setter("LightingOriginEntity", "__lighting_origin"); getter("LightingOriginEntity", "__lighting_origin", NULL)
setter("BoneController", "__bone_controller")
function ENT:SetBoneController(i, v) local t = evars(self); t.__bone_controllers = t.__bone_controllers or {}; t.__bone_controllers[i] = v end
function ENT:GetBoneController(i) local t = evars(self).__bone_controllers; return t and t[i] or 0 end
function ENT:DrawShadow(on) evars(self).__draw_shadow = on ~= false end
function ENT:SetIK(on) evars(self).__ik = on ~= false end
function ENT:UseClientSideAnimation(on) evars(self).__client_anim = on ~= false end
function ENT:EnableConstraints(on) evars(self).__constraints_enabled = on ~= false end
function ENT:SetSurroundingBounds(mins, maxs) local v = evars(self); v.__surround_mins, v.__surround_maxs = mins, maxs end
function ENT:SetSurroundingBoundsType(t) evars(self).__surround_type = t end
function ENT:SetParentPhysNum(n) evars(self).__parent_phys = n end
function ENT:SetPhysConstraintObjects(a, b) local v = evars(self); v.__constraint_objs = { a, b } end
function ENT:SetRagdollPos(p) evars(self).__ragdoll_pos = p end
function ENT:SetRagdollAng(a) evars(self).__ragdoll_ang = a end
function ENT:SetRagdollBuildFunction(fn) evars(self).__ragdoll_build = fn end
function ENT:SetEntity(name, ent) local v = evars(self); v.__named_ents = v.__named_ents or {}; v.__named_ents[name] = ent end
function ENT:GetEditingData() return evars(self).__edit or {} end
function ENT:SetHitboxSet(set) evars(self).__hitbox_set = set end
local base_hitbox_set = ENT.GetHitboxSet
function ENT:GetHitboxSet() local s = evars(self).__hitbox_set; if s ~= nil then return tonumber(s) or 0, tostring(s) end return base_hitbox_set(self) end
function ENT:SetLayerBlendIn(layer, t) local v = evars(self); v.__layer_blend_in = v.__layer_blend_in or {}; v.__layer_blend_in[layer] = t end
function ENT:SetLayerBlendOut(layer, t) local v = evars(self); v.__layer_blend_out = v.__layer_blend_out or {}; v.__layer_blend_out[layer] = t end
function ENT:MakePhysicsObjectAShadow(allowMove, allowRot) local v = evars(self); v.__shadow = { move = allowMove, rot = allowRot }; local p = self:GetPhysicsObject(); if IsValid(p) then p:EnableGravity(false) end end
function ENT:SetAttachment(parent, attachment) self:SetParent(parent); evars(self).__parent_attachment = attachment end
function ENT:FollowBone(parent, bone) self:SetParent(parent); evars(self).__follow_bone = bone end
function ENT:GetPreferredCarryAngles() return evars(self).__carry_angles or (self.PreferredCarryAngles) or Angle(0, 0, 0) end
function ENT:GetRagdollOwner() return self:GetOwner() end
function ENT:GetMomentaryRotButtonPos() return evars(self).__rot_button_pos or 0 end
function ENT:BodyTarget(origin, noisy) return self:WorldSpaceCenter() end
function ENT:HeadTarget(origin) if self.EyePos then return self:EyePos() end return self:WorldSpaceCenter() end
function ENT:AlignAngles(from, to) self:SetAngles(self:GetAngles() + (to - from)) end
function ENT:Blocked(other) end
function ENT:BoneLength(b)
    local p = self:GetBoneParent(b)
    if p == nil or p < 0 then return 0 end
    local a, c = self:GetBonePosition(b), self:GetBonePosition(p)
    if a and c then return a:Distance(c) end
    return 0
end
function ENT:BoneHasFlag(b, flag) return b ~= nil and b >= 0 and b < self:GetBoneCount() and flag ~= 0 end
function ENT:GetBrushPlaneCount() return 0 end
function ENT:GetBrushPlane(i) return nil end
function ENT:GetBrushSurfaces() return {} end
function ENT:FindTransitionSequence(current, goal) return goal end
function ENT:MuzzleFlash()
    if util and util.Effect and EffectData then
        local ed = EffectData(); ed:SetEntity(self); ed:SetAttachment(1); ed:SetOrigin(self:GetPos())
        util.Effect("MuzzleFlash", ed)
    end
end
function ENT:GibBreakClient(force, clr) evars(self).__gibbed = true end
function ENT:GibBreakServer(force) evars(self).__gibbed = true end
function ENT:PrecacheGibs() return 0 end
function ENT:PlayScene(scene, delay) evars(self).__scene = scene; return 0, 0 end
function ENT:RagdollSolve() end
function ENT:RagdollStopControlling() end
function ENT:RagdollUpdatePhysics() end
function ENT:CreateBoneFollowers() end
function ENT:DestroyBoneFollowers() end
function ENT:UpdateBoneFollowers() end
function ENT:RemoveInternalConstraint(i) local v = evars(self); v.__removed_internal = v.__removed_internal or {}; v.__removed_internal[i or 0] = true end
function ENT:SetBoneMatrix(b, m) local v = evars(self); v.__bone_matrix = v.__bone_matrix or {}; v.__bone_matrix[b] = m end
function ENT:CopyBoneMatrix(b, m) local src = self:GetBoneMatrix(b); if src and m and m.Set then m:Set(src) end end
local base_bone_matrix = ENT.GetBoneMatrix
function ENT:GetBoneMatrix(b) local v = rawget(self, "__vars"); local m = v and v.__bone_matrix and v.__bone_matrix[b]; if m then return m end return base_bone_matrix(self, b) end
function ENT:SendViewModelMatchingSequence(seq) if type(seq) == "number" and seq >= 0 then self:SetSequence(seq) end end
function ENT:ViewModelIndex() return evars(self).__vm_index or 0 end
-- motion controller: `PhysicsSimulate` runs for every registered object each tick and its
-- angular / linear results are applied as the SIM_* mode says (the game's IMotionEvent)
function ENT:AddToMotionController(phys) local v = evars(self); v.__motion_objs = v.__motion_objs or {}; table.insert(v.__motion_objs, phys) end
function ENT:RemoveFromMotionController(phys) local v = evars(self); if not v.__motion_objs then return end for i, p in ipairs(v.__motion_objs) do if p == phys then table.remove(v.__motion_objs, i) return end end end
function ENT:StartMotionController()
    local v = evars(self); v.__motion = true
    if not v.__motion_objs or #v.__motion_objs == 0 then v.__motion_objs = { self:GetPhysicsObject() } end
    local me = self
    hook.Add("Tick", "__motion_" .. tostring(self.__id), function()
        if not IsValid(me) or not evars(me).__motion then hook.Remove("Tick", "__motion_" .. tostring(me.__id)) return end
        local dt = FrameTime()
        for _, p in ipairs(evars(me).__motion_objs or {}) do
            if IsValid(p) and me.PhysicsSimulate then
                local ang, lin, mode = me:PhysicsSimulate(p, dt)
                -- a mode below SIM_LOCAL_ACCELERATION (1) ignores both vectors (toolgun.md F57)
                mode = tonumber(mode) or 0
                if mode < 1 then lin, ang = nil, nil end
                if lin and lin ~= 0 then
                    if mode == (SIM_GLOBAL_ACCELERATION or 3) then p:AddVelocity(lin * dt)
                    elseif mode == (SIM_GLOBAL_FORCE or 2) then p:ApplyForceCenter(lin)
                    elseif mode == (SIM_LOCAL_ACCELERATION or 1) then p:AddVelocity(p:LocalToWorldVector(lin) * dt)
                    else p:ApplyForceCenter(p:LocalToWorldVector(lin)) end
                end
                if ang and ang ~= 0 then
                    if mode == (SIM_GLOBAL_ACCELERATION or 3) or mode == (SIM_LOCAL_ACCELERATION or 1) then p:AddAngleVelocity(ang * dt)
                    else p:AddAngleVelocity(ang / math.max(p:GetMass(), 1e-3) * dt) end
                end
            end
        end
    end)
end
function ENT:StopMotionController() local v = evars(self); v.__motion = false; hook.Remove("Tick", "__motion_" .. tostring(self.__id)) end
local function points_bounds(points)
    local mins, maxs = Vector(math.huge, math.huge, math.huge), Vector(-math.huge, -math.huge, -math.huge)
    local n = 0
    for _, p in ipairs(points) do
        if type(p) == "table" and p[1] then for _, q in ipairs(p) do mins.x = math.min(mins.x, q.x); mins.y = math.min(mins.y, q.y); mins.z = math.min(mins.z, q.z); maxs.x = math.max(maxs.x, q.x); maxs.y = math.max(maxs.y, q.y); maxs.z = math.max(maxs.z, q.z); n = n + 1 end
        else local q = p.pos or p; mins.x = math.min(mins.x, q.x); mins.y = math.min(mins.y, q.y); mins.z = math.min(mins.z, q.z); maxs.x = math.max(maxs.x, q.x); maxs.y = math.max(maxs.y, q.y); maxs.z = math.max(maxs.z, q.z); n = n + 1 end
    end
    if n == 0 then return Vector(-1, -1, -1), Vector(1, 1, 1) end
    return mins, maxs
end
-- convex / mesh collision falls back to the hull's box (the physics backend builds convex hulls from the model only)
function ENT:PhysicsInitConvex(points) local mins, maxs = points_bounds(points or {}); evars(self).__phys_points = points; return self:PhysicsInitBox(mins, maxs) end
function ENT:PhysicsInitMultiConvex(groups) local mins, maxs = points_bounds(groups or {}); evars(self).__phys_points = groups; return self:PhysicsInitBox(mins, maxs) end
function ENT:PhysicsFromMesh(verts) local mins, maxs = points_bounds(verts or {}); evars(self).__phys_mesh = verts; return self:PhysicsInitBox(mins, maxs) end

local function pvars(self) local v = rawget(self, "__pv"); if not v then v = {}; rawset(self, "__pv", v) end return v end
function P:SetDragCoefficient(c) qwrite("set_drag_coef", self.__ent.__id, { linear = tonumber(c) or 0 }) end
function P:SetAngleDragCoefficient(c) qwrite("set_drag_coef", self.__ent.__id, { angular = tonumber(c) or 0 }) end
function P:AddGameFlag(f) local v = pvars(self); v.flags = bit.bor(v.flags or 0, f) end
function P:ClearGameFlag(f) local v = pvars(self); v.flags = bit.band(v.flags or 0, bit.bnot(f)) end
function P:HasGameFlag(f) return bit.band(pvars(self).flags or 0, f) ~= 0 end
function P:GetContents() local c = pvars(self).contents; if c == nil then return CONTENTS_SOLID end return c end
function P:SetContents(c) pvars(self).contents = c end
function P:GetBuoyancyRatio() return pvars(self).buoyancy or 1 end
function P:SetBuoyancyRatio(r) pvars(self).buoyancy = r end
function P:GetShadowPos() local v = pvars(self); return v.shadow_pos or self:GetPos() end
function P:GetShadowAngles() local v = pvars(self); return v.shadow_ang or self:GetAngles() end
function P:UpdateShadow(pos, ang, dt) local v = pvars(self); v.shadow_pos, v.shadow_ang = pos, ang end
function P:ComputeShadowControl(params)
    local v = pvars(self)
    local dt = params.deltatime or FrameTime()
    local arrive = math.max(params.secondstoarrive or dt, 1e-3)
    local pos, ang = params.pos or self:GetPos(), params.angle or self:GetAngles()
    v.shadow_pos, v.shadow_ang = pos, ang
    local cur = self:GetPos()
    local delta = pos - cur
    if (params.teleportdistance or 0) > 0 and delta:Length() > params.teleportdistance then self:SetPos(pos); self:SetAngles(ang); self:SetVelocity(Vector(0, 0, 0)); self:SetAngleVelocity(Vector(0, 0, 0)); return 0 end
    local vel = delta / arrive
    local maxs = params.maxspeed or 0
    if maxs > 0 and vel:Length() > maxs then vel = vel:GetNormalized() * maxs end
    local damp = math.Clamp(params.dampfactor or 1, 0, 1)
    self:SetVelocity(self:GetVelocity() * (1 - damp) + vel * damp)
    local ca = self:GetAngles()
    local da = Vector(math.AngleDifference(ang.p, ca.p), math.AngleDifference(ang.y, ca.y), math.AngleDifference(ang.r, ca.r)) / arrive
    local maxa = params.maxangular or 0
    if maxa > 0 and da:Length() > maxa then da = da:GetNormalized() * maxa end
    self:SetAngleVelocity(self:GetAngleVelocity() * (1 - damp) + da * damp)
    return arrive
end
function P:GetStress() return 0, 0 end
function P:IsPenetrating() return false end
function P:GetFrictionSnapshot() return {} end
function P:RecheckCollisionFilter() end

function VEH:GetAmmo() return -1, 0, 0 end
function VEH:GetCameraDistance() return evars(self).__cam_dist or -1 end
function VEH:SetCameraDistance(d) evars(self).__cam_dist = d end
function VEH:GetThirdPersonMode() return evars(self).__third_person == true end
function VEH:SetThirdPersonMode(on) evars(self).__third_person = on == true end
function VEH:GetWheel(i) return nil end
function VEH:HasBrakePedal() local v = evars(self).__brake_pedal; if v == nil then return true end return v end
function VEH:SetHasBrakePedal(on) evars(self).__brake_pedal = on ~= false end
local base_vehicle_class = VEH.GetVehicleClass
function VEH:SetVehicleClass(c) evars(self).__vehicle_class = c end
function VEH:GetVehicleClass() local c = evars(self).__vehicle_class; if c then return c end return base_vehicle_class and base_vehicle_class(self) or "" end
function VEH:SetVehicleEntryAnim(on) evars(self).__entry_anim = on ~= false end

function WEP:CallOnClient(fn, args) local f = self[fn]; if type(f) == "function" then f(self, args) end end
function WEP:GetActivity() return evars(self).__activity or ACT_INVALID or -1 end
function WEP:SetActivity(a) evars(self).__activity = a end
function WEP:IsWeaponVisible() local o = self:GetOwner(); if not IsValid(o) then return true end return o:Alive() and o:GetActiveWeapon() == self end
function WEP:SendWeaponAnim(act)
    evars(self).__activity = act
    if self.SelectWeightedSequence and self.SetSequence then local s = self:SelectWeightedSequence(act); if s and s >= 0 then self:SetSequence(s) end end
end

------------------------------------------------------------------------ Player: suit, hands / viewmodels, observer, walking / zoom, flashlight colour, DSP, ragdoll, voice, widgets
local function plv(self) local v = rawget(self, "__vars"); if not v then v = {}; rawset(self, "__vars", v) end return v end
-- a non-networked entity owned by a player (the game's predicted_viewmodel / gmod_hands)
local VENT = {}
VENT.__index = VENT
VENT.__tostring = function(self) return "Entity [-1][" .. self.__class .. "]" end
local function new_vent(class, owner, model)
    return setmetatable({ __valid = true, __class = class, __owner = owner, __model = model or "", __skin = 0, __body = {}, __nodraw = false, __vars = {}, __parent = owner, __wep_model = "" }, VENT)
end
function VENT:IsValid() return self.__valid == true end
function VENT:Remove() self.__valid = false end
function VENT:EntIndex() return -1 end
function VENT:GetClass() return self.__class end
function VENT:GetOwner() return self.__owner end
function VENT:GetParent() return self.__parent end
function VENT:SetParent(p) self.__parent = p end
function VENT:GetModel() return self.__model end
function VENT:SetModel(m) self.__model = m or "" end
function VENT:GetSkin() return self.__skin end
function VENT:SetSkin(s) self.__skin = s or 0 end
function VENT:GetBodygroup(i) return self.__body[i] or 0 end
function VENT:SetBodygroup(i, v) self.__body[i] = v end
function VENT:SetBodyGroups(s) for i = 1, #tostring(s) do self.__body[i - 1] = tonumber(string.sub(s, i, i)) or 0 end end
function VENT:GetPos() local o = self.__owner; if IsValid(o) then return o:EyePos() end return Vector(0, 0, 0) end
function VENT:GetAngles() local o = self.__owner; if IsValid(o) then return o:EyeAngles() end return Angle(0, 0, 0) end
function VENT:SetPos() end
function VENT:SetAngles() end
function VENT:SetNoDraw(b) self.__nodraw = b == true end
function VENT:GetNoDraw() return self.__nodraw end
function VENT:IsPlayer() return false end
function VENT:IsNPC() return false end
function VENT:IsWeapon() return false end
function VENT:IsWorld() return false end
function VENT:IsVehicle() return false end
function VENT:GetTable() return self.__vars end
function VENT:GetWeaponModel() return self.__wep_model end
function VENT:SetWeaponModel(m, wep) self.__wep_model = m or ""; self.__weapon = wep end
function VENT:GetWeapon() return self.__weapon or NULL end
function VENT:GetSequence() return self.__seq or 0 end
function VENT:SetSequence(s) self.__seq = s end
function VENT:SendViewModelMatchingSequence(s) self.__seq = s end
function VENT:ResetSequence(s) self.__seq = s end
function VENT:SetPlaybackRate(r) self.__rate = r end
function VENT:GetPlaybackRate() return self.__rate or 1 end
function VENT:SetColor(c) self.__color = c end
function VENT:GetColor() return self.__color or Color(255, 255, 255) end
function VENT:SetMaterial(m) self.__material = m end
function VENT:GetMaterial() return self.__material or "" end
function VENT:SetRenderMode(m) self.__render_mode = m end
function VENT:GetRenderMode() return self.__render_mode or 0 end
function VENT:LookupAttachment() return 0 end
function VENT:GetAttachment() return nil end
function VENT:LookupBone() return nil end
function VENT:GetBoneCount() return 0 end
function VENT:SetPoseParameter() end
function VENT:GetPoseParameter() return 0 end
function PLY:GetViewModel(i)
    local v = plv(self); v.__viewmodels = v.__viewmodels or {}
    i = i or 0
    if not v.__viewmodels[i] then
        local vm = new_vent("viewmodel", self, "")
        local w = self:GetActiveWeapon()
        if IsValid(w) and w.GetWeaponViewModel then vm.__model = w:GetWeaponViewModel() or "" end
        v.__viewmodels[i] = vm
    end
    return v.__viewmodels[i]
end
function PLY:GetHands() local v = plv(self); if not v.__hands then v.__hands = new_vent("gmod_hands", self, "models/weapons/c_arms_citizen.mdl") end return v.__hands end
function PLY:SetHands(h) plv(self).__hands = h end
function PLY:SetupHands()
    local hands = self:GetHands()
    if not IsValid(hands) then return end
    local pm = player_manager
    local info = pm and pm.TranslatePlayerHands and pm.TranslatePlayerHands(pm.TranslateToPlayerModelName and pm.TranslateToPlayerModelName(self:GetModel()) or self:GetModel())
    if info then hands:SetModel(info.model or hands:GetModel()); hands:SetSkin(info.skin or 0); hands:SetBodyGroups(info.body or "0000000") end
    hands.__weapon = self:GetActiveWeapon()
end
function PLY:GetSuitPower() return plv(self).__suit_power or 100 end
function PLY:SetSuitPower(p) plv(self).__suit_power = math.Clamp(tonumber(p) or 0, 0, 100) end
-- observer / spectate: stored mode and target, the view entity is the target while spectating
local OBS_NONE = OBS_MODE_NONE or 0
function PLY:SetObserverMode(m) plv(self).__observer = m or OBS_NONE end
local base_obs = PLY.GetObserverMode
function PLY:GetObserverMode() local m = plv(self).__observer; if m ~= nil then return m end return base_obs(self) end
function PLY:Spectate(m) local v = plv(self); v.__observer = m or OBS_MODE_ROAMING or 6; v.__spectating = true end
function PLY:SpectateEntity(e) plv(self).__spectate_target = e end
function PLY:GetObserverTarget() local t = plv(self).__spectate_target; if t ~= nil and IsValid(t) then return t end return NULL end
function PLY:UnSpectate() local v = plv(self); v.__observer = OBS_NONE; v.__spectating = false; v.__spectate_target = nil end
function PLY:SetViewEntity(e) plv(self).__view_entity = e end
function PLY:GetViewEntity()
    local e = plv(self).__view_entity
    if e ~= nil and IsValid(e) then return e end
    local t = self:GetObserverTarget()
    if IsValid(t) and self:GetObserverMode() ~= OBS_NONE then return t end
    return self
end
local function pset(name, key) PLY["Set" .. name] = function(self, val) plv(self)[key] = val end end
local function pget(name, key, default) PLY["Get" .. name] = function(self) local v = plv(self)[key]; if v == nil then return default end return v end end
pset("CanWalk", "__can_walk"); pget("CanWalk", "__can_walk", true)
pset("CanZoom", "__can_zoom"); pget("CanZoom", "__can_zoom", true)
pset("FlashlightColor", "__flashlight_color"); pget("FlashlightColor", "__flashlight_color", Vector(1, 1, 1))
pset("AllowFullRotation", "__full_rot"); pget("AllowFullRotation", "__full_rot", false)
pset("AllowWeaponsInVehicle", "__wep_vehicle"); pget("AllowWeaponsInVehicle", "__wep_vehicle", false)
pset("AvoidPlayers", "__avoid"); pget("AvoidPlayers", "__avoid", true)
pset("ClassID", "__class_id"); pget("ClassID", "__class_id", 0)
pset("NoCollideWithTeammates", "__nocollide_team"); pget("NoCollideWithTeammates", "__nocollide_team", false)
pset("Activity", "__activity"); pget("Activity", "__activity", ACT_INVALID or -1)
pset("HoveredWidget", "__hovered_widget"); pget("HoveredWidget", "__hovered_widget", NULL)
pset("PressedWidget", "__pressed_widget"); pget("PressedWidget", "__pressed_widget", NULL)
pset("VoiceVolumeScale", "__voice_scale"); pget("VoiceVolumeScale", "__voice_scale", 1)
pset("SuppressPickupNotices", "__suppress_pickup")
pset("CurrentViewOffset", "__cur_view_offset")
function PLY:SetDSP(dsp, fast) plv(self).__dsp = dsp end
function PLY:SetRenderAngles(a) plv(self).__render_angles = a end
function PLY:GetRenderAngles() return plv(self).__render_angles or self:GetAngles() end
function PLY:SetDrivingEntity(e, mode) local v = plv(self); v.__driving, v.__driving_mode = e, mode end
local base_driving = PLY.GetDrivingEntity
function PLY:GetDrivingEntity() local e = plv(self).__driving; if e ~= nil and IsValid(e) then return e end return base_driving(self) end
function PLY:StartWalking() plv(self).__walking = true end
function PLY:StopWalking() plv(self).__walking = false end
local base_walking = PLY.IsWalking
function PLY:IsWalking() if plv(self).__walking then return true end return base_walking(self) end
function PLY:StopZooming() self:SetFOV(0, 0) end
function PLY:SetMuted(m) plv(self).__muted = m == true end
function PLY:IsMuted() return plv(self).__muted == true end
function PLY:VoiceVolume() return 0 end
function PLY:IsVoiceAudible() return false end
function PLY:IsPlayingTaunt() return plv(self).__taunt == true end
function PLY:MotionSensorPos(bone) return Vector(0, 0, 0) end
function PLY:ShouldDrawLocalPlayer() return plv(self).__draw_local == true end
function PLY:GetFriendStatus() return "none" end
function PLY:GetPlayerInfo() return { name = self:Nick(), userid = self:UserID(), friendsid = 0, fakeplayer = self:IsBot(), ishltv = false, networkid = self.SteamID and self:SteamID() or "" } end
function PLY:GetPreferredCarryAngles(ent) if IsValid(ent) and ent.GetPreferredCarryAngles then return ent:GetPreferredCarryAngles() end return Angle(0, 0, 0) end
function PLY:ShowProfile() end
function PLY:SuppressHint(name) local v = plv(self); v.__hints = v.__hints or {}; v.__hints[name] = true end
function PLY:AddPlayerOption(name, timeout, vote, draw) return false end
function PLY:AllowImmediateDecalPainting(on) plv(self).__decal_now = on ~= false end
function PLY:AnimRestartMainSequence() if self.GetSequence and self.ResetSequence then self:ResetSequence(self:GetSequence()) end end
function PLY:AddVCDSequenceToGestureSlot(slot, seq, cycle, autokill) if self.AnimRestartGesture then self:AnimRestartGesture(slot, seq, autokill) end end
function PLY:TranslateWeaponActivity(act) local w = self:GetActiveWeapon(); if IsValid(w) and w.TranslateActivity then local t = w:TranslateActivity(act); if t and t >= 0 then return t end end return act end
function PLY:CrosshairDisable() plv(self).__crosshair_off = true end
function PLY:CrosshairEnable() plv(self).__crosshair_off = false end
function PLY:DisableWorldClicking(b) plv(self).__no_world_click = b == true end
function PLY:IsWorldClickingDisabled() return plv(self).__no_world_click == true end
function PLY:DrawViewModel(b, i) local vm = self:GetViewModel(i); if vm then vm:SetNoDraw(b == false) end end
function PLY:DrawWorldModel(b) plv(self).__draw_world_model = b ~= false end
function PLY:GetRagdollEntity()
    local r = plv(self).__ragdoll
    if r ~= nil and IsValid(r) then return r end
    local p = psnap(self.__pid)
    if p and (p.ragdoll or 0) ~= 0 then return Entity(p.ragdoll) end
    return NULL
end
function PLY:CreateRagdoll()
    local r = ents.Create("prop_ragdoll")
    if not IsValid(r) then return end
    r:SetModel(self:GetModel()); r:SetPos(self:GetPos()); r:SetAngles(self:GetAngles()); r:SetSkin(self:GetSkin()); r:Spawn()
    r:SetOwner(self)
    plv(self).__ragdoll = r
end
function PLY:LagCompensation(on) plv(self).__lag_comp = on == true end
function PLY:SprayDecal()
    local tr = self:GetEyeTrace()
    if tr and tr.Hit and util and util.Decal then util.Decal("Spray", tr.HitPos + tr.HitNormal, tr.HitPos - tr.HitNormal, self) end
end
function PLY:SimulateGravGunPickup(ent, lightning) if IsValid(ent) and ent.GravGunOnPickedUp then ent:GravGunOnPickedUp(self) end hook.Run("GravGunOnPickedUp", self, ent) end
function PLY:SimulateGravGunDrop(ent) if IsValid(ent) and ent.GravGunOnDropped then ent:GravGunOnDropped(self) end hook.Run("GravGunOnDropped", self, ent) end
function PLY:DetonateTripmines() for _, e in ipairs(ents.FindByClass("npc_tripmine")) do if e:GetOwner() == self then e:Fire("Explode", "", 0) end end end

-- ---------------------------------------------------------------- HL2 brain: schedules / conditions
-- The server runs the NPC's schedule (engine tasks over the native goto / chase / cover / stop
-- operations) and gathers conditions every think; SetSchedule hands it a schedule, ClearSchedule
-- gives the native decisions back. Conditions set from script live until the next gather.
local function ai_snap(self) return snap(self.__id) or {} end
function NPC:SetSchedule(sched) qwrite("npc_ai_schedule", self.__id, { sched = sched }) end
function NPC:ClearSchedule() qwrite("npc_ai_schedule", self.__id, { sched = nil }) end
function NPC:GetCurrentSchedule() local s = ai_snap(self); return s.npc_ai_schedule or SCHED_NONE end
function NPC:IsCurrentSchedule(sched) return self:GetCurrentSchedule() == sched end
function NPC:GetTaskStatus() local s = ai_snap(self); return s.npc_ai_task_status or TASKSTATUS_NEW end
function NPC:SetTaskStatus(status) qwrite("npc_ai_task_status", self.__id, { status = status }) end
function NPC:TaskComplete() qwrite("npc_ai_task_status", self.__id, { status = TASKSTATUS_COMPLETE }) end
function NPC:TaskFail(task) qwrite("npc_ai_task_fail", self.__id) end
function NPC:HasCondition(cond)
    local s = ai_snap(self)
    for _, c in ipairs(s.npc_ai_conditions or {}) do if c == cond then return true end end
    return false
end
function NPC:SetCondition(cond) qwrite("npc_ai_condition", self.__id, { cond = cond, on = true }) end
function NPC:ClearCondition(cond) qwrite("npc_ai_condition", self.__id, { cond = cond, on = false }) end
function NPC:ConditionName(cond) return __glua_cond_names and __glua_cond_names[cond] or "" end
function NPC:ConditionID(name) for id, n in pairs(__glua_cond_names or {}) do if n == name then return id end end return -1 end
function NPC:SetIgnoreConditions(conds, duration)
    qwrite("npc_ai_ignore_conds", self.__id, { conds = conds })
    if duration and duration > 0 and timer then
        local id = self.__id
        timer.Simple(duration, function() qwrite("npc_ai_ignore_conds", id, { conds = conds, clear = true }) end)
    end
end
function NPC:RemoveIgnoreConditions(conds) qwrite("npc_ai_ignore_conds", self.__id, { conds = conds, clear = true }) end
function NPC:SetNPCState(state) qwrite("npc_ai_state", self.__id, { state = state }) end
function NPC:GetCurrentWeaponProficiency() local s = ai_snap(self); return s.npc_ai_proficiency or WEAPON_PROFICIENCY_POOR end
function NPC:SetCurrentWeaponProficiency(p) qwrite("npc_ai_proficiency", self.__id, { level = p }) end
function NPC:IsUnforgettable() return ai_snap(self).npc_ai_unforgettable == true end
function NPC:SetUnforgettable(on) qwrite("npc_ai_misc", self.__id, { unforgettable = on ~= false }) end
function NPC:GetExpression() return ai_snap(self).npc_ai_expression or "" end
function NPC:SetExpression(scene) qwrite("npc_ai_misc", self.__id, { expression = tostring(scene or "") }); return 0 end
function NPC:ClearExpression() qwrite("npc_ai_misc", self.__id, { expression = "" }) end
function NPC:SetMaxRouteRebuildTime(t) qwrite("npc_ai_misc", self.__id, { route_rebuild = t }) end
function NPC:GetMoveDelay() return rawget(self, "__move_delay") or 0 end
function NPC:SetMoveDelay(t) rawset(self, "__move_delay", t); qwrite("npc_ai_misc", self.__id, { move_delay = t }) end
function NPC:IsRunningBehavior() return ai_snap(self).npc_ai_behavior ~= nil end
local function use_behavior(self, name) qwrite("npc_ai_misc", self.__id, { behavior_set = true, behavior = name }) end
function NPC:UseActBusyBehavior() use_behavior(self, "ActBusy") end
function NPC:UseAssaultBehavior() use_behavior(self, "Assault") end
function NPC:UseFollowBehavior() use_behavior(self, "Follow") end
function NPC:UseFuncTankBehavior() use_behavior(self, "FuncTank") end
function NPC:UseLeadBehavior() use_behavior(self, "Lead") end
function NPC:UseNoBehavior() use_behavior(self, nil) end
-- the ai_goal_* logic entities (scripted.luau) hand the server a goal for the behaviour
function NPC:__SetAiGoal(goal) qwrite("npc_ai_goal", self.__id, { json = util.TableToJSON(goal) }) end
function NPC:__GetAiGoalStage() return ai_snap(self).npc_ai_goal_stage end
-- ENTITY:GetSoundInterests of an ai-type scripted entity: the SOUND_* bits it listens to
function ENT:__SetSoundInterests(bits) qwrite("npc_ai_misc", self.__id, { sound_interests_set = true, sound_interests = bits }) end
function NPC:__SetAiScript(script) qwrite("npc_ai_script", self.__id, { json = util.TableToJSON(script) }) end
function NPC:__GetAiScriptStage() return ai_snap(self).npc_ai_script_stage end
function NPC:CapabilitiesAdd(c) qwrite("npc_caps", self.__id, { caps = bit.bor(self:CapabilitiesGet(), c or 0) }) end
function NPC:CapabilitiesRemove(c) qwrite("npc_caps", self.__id, { caps = bit.band(self:CapabilitiesGet(), bit.bnot(c or 0)) }) end
function NPC:CapabilitiesClear() qwrite("npc_caps", self.__id, { caps = 0 }) end
function NPC:SetFOV(deg) rawset(self, "__fov", deg) end
local base_fov = NPC.GetFOV
function NPC:GetFOV() return rawget(self, "__fov") or base_fov(self) end
-- activity / sequence bookkeeping the engine keeps on the brain (the body plays what the
-- native animation picks; these hold the ideal the script asked for)
function NPC:GetIdealActivity() return rawget(self, "__ideal_act") or self:GetActivity() end
function NPC:SetIdealActivity(a) rawset(self, "__ideal_act", a) end
function NPC:ResetIdealActivity(a) rawset(self, "__ideal_act", a) end
function NPC:SetActivity(a) rawset(self, "__ideal_act", a); if a ~= nil and self.SetSequence and self.SelectWeightedSequence then local seq = self:SelectWeightedSequence(a); if seq and seq >= 0 then self:SetSequence(seq) end end end
function NPC:MaintainActivity() local a = rawget(self, "__ideal_act"); if a ~= nil and a ~= self:GetActivity() and self.SelectWeightedSequence and self.SetSequence then local seq = self:SelectWeightedSequence(a); if seq and seq >= 0 then self:SetSequence(seq) end end end
function NPC:GetIdealSequence() return rawget(self, "__ideal_seq") or (self.GetSequence and self:GetSequence()) or 0 end
function NPC:SetIdealSequence(seq) rawset(self, "__ideal_seq", seq) end
function NPC:GetMovementActivity() return self:GetVelocity():Length() > 1 and (rawget(self, "__move_act") or ACT_RUN) or ACT_IDLE end
function NPC:GetMovementSequence() return rawget(self, "__move_seq") or (self.GetSequence and self:GetSequence()) or 0 end
function NPC:SetMovementSequence(seq) rawset(self, "__move_seq", seq) end
function NPC:GetIdealMoveAcceleration() return 500 end
function NPC:GetMinMoveCheckDist() return 36 end
function NPC:GetMinMoveStopDist() return 10 end
function NPC:UpdateTurnActivity() local y = self:GetIdealYaw() - self:GetAngles().y; y = ((y + 180) % 360) - 180; if math.abs(y) > 45 then rawset(self, "__ideal_act", y > 0 and ACT_TURN_LEFT or ACT_TURN_RIGHT) end end
-- arrival: what the NPC does at the end of the current path
function NPC:GetArrivalActivity() return rawget(self, "__arr_act") or ACT_INVALID end
function NPC:SetArrivalActivity(a) rawset(self, "__arr_act", a) end
function NPC:GetArrivalSequence() return rawget(self, "__arr_seq") or -1 end
function NPC:SetArrivalSequence(s) rawset(self, "__arr_seq", s) end
function NPC:GetArrivalDirection() return rawget(self, "__arr_dir") or Vector(0, 0, 0) end
function NPC:SetArrivalDirection(d) rawset(self, "__arr_dir", d) end
function NPC:GetArrivalSpeed() return rawget(self, "__arr_speed") or 0 end
function NPC:SetArrivalSpeed(s) rawset(self, "__arr_speed", s) end
function NPC:ResetMoveCalc() rawset(self, "__arr_act", nil); rawset(self, "__arr_seq", nil); rawset(self, "__arr_dir", nil); rawset(self, "__arr_speed", nil) end
function NPC:AdvancePath() local s = ai_snap(self); return s.npc_goal ~= nil end
function NPC:AutoMovement(interval, target) local s = ai_snap(self); return s.npc_goal ~= nil or (s.npc_move_speed or 0) > 0 end
function NPC:MoveGroundStep(move, target) if move then self:NavSetGoalPos(self:GetPos() + move) end return 0 end
function NPC:MoveJumpStart(vel) if self.SetVelocity then self:SetVelocity(vel) end end
function NPC:MoveJumpExec() return self:IsOnGround() and 0 or 1 end
function NPC:MoveJumpStop() return 0 end
function NPC:MoveClimbStart(dest, dir, yaw) self:NavSetGoalPos(dest) end
function NPC:MoveClimbExec(dest, dir, yaw, left) return self:GetPos():Distance(dest) < 16 and 0 or 1 end
function NPC:MoveClimbStop() return 0 end
function NPC:GetBlockingEntity() local b = rawget(self, "__blocking"); if b and IsValid(b) then return b end return NULL end
function NPC:ClearBlockingEntity() rawset(self, "__blocking", nil) end
function NPC:GetBestSoundHint(types) return NULL end
function NPC:ExitScriptedSequence() self:ClearSchedule(); return true end
function NPC:BecomeRagdoll(dmg) self:TakeDamageInfo(dmg or DamageInfo()); if self.Kill then self:Kill() end return self.GetRagdollEntity and self:GetRagdollEntity() or NULL end
-- the sentence sounds, played through EmitSound by class as the game's sound scripts name them
local function npc_sound(self, kind)
    local c = self:GetClass()
    local base = ({ npc_combine_s = "NPC_Combine", npc_metropolice = "NPC_MetroPolice", npc_zombie = "Zombie", npc_fastzombie = "NPC_FastZombie", npc_poisonzombie = "NPC_PoisonZombie", npc_headcrab = "NPC_HeadCrab", npc_antlion = "NPC_Antlion", npc_citizen = "NPC_Citizen", npc_vortigaunt = "NPC_Vortigaunt" })[c]
    if base and self.EmitSound then self:EmitSound(base .. "." .. kind) end
end
function NPC:IdleSound() npc_sound(self, "Idle") end
function NPC:AlertSound() npc_sound(self, "Alert") end
function NPC:FearSound() npc_sound(self, "Fear") end
function NPC:FoundEnemySound() npc_sound(self, "FoundEnemy") end
function NPC:LostEnemySound() npc_sound(self, "LostEnemy") end
function NPC:PlaySentence(name, delay, volume) if self.EmitSound then self:EmitSound(tostring(name), 75, 100, volume or 1) end return 0 end
function NPC:SentenceStop() if self.StopSound then self:StopSound("") end end
function NPC:StartEngineTask(task, data) qwrite("npc_ai_task_status", self.__id, { status = TASKSTATUS_RUN_TASK }) end
function NPC:RunEngineTask(task, data) end
-- schedule-local save position the FORCED_GO / SAVEPOSITION tasks walk to
function NPC:SetLastPosition(pos) rawset(self, "__last_pos", pos); qwrite("npc_ai_save_pos", self.__id, { x = pos.x, y = pos.y, z = pos.z }) end

-- engine-side ENTITY callbacks: the defaults keep the engine behaviour (no custom position, mesh, collision or impact)
-- @hook ENTITY:CalcAbsolutePosition
function ENT:CalcAbsolutePosition(pos, ang) return pos, ang end
-- @hook ENTITY:FireAnimationEvent
function ENT:FireAnimationEvent(pos, ang, event, name, caller) return false end
-- @hook ENTITY:GetRenderMesh
function ENT:GetRenderMesh() return nil end
-- @hook ENTITY:GetShadowCastDirection
function ENT:GetShadowCastDirection(dir) return dir end
-- @hook ENTITY:ImpactTrace
function ENT:ImpactTrace(tr, damageType, customImpactName) return false end
-- @hook ENTITY:OnTraceAttack
function ENT:OnTraceAttack(dmginfo, dir, tr) end
-- @hook ENTITY:ResolveCustomFlyCollision
function ENT:ResolveCustomFlyCollision(tr) end
-- @hook ENTITY:TestCollision
function ENT:TestCollision(startpos, delta, isbox, extents, mask) return nil end
