-- Dispatched scripted callbacks (counted in the coverage manifest):
-- @hook ENTITY:Initialize
-- @hook ENTITY:Think
-- @hook ENTITY:StartTouch
-- @hook ENTITY:Touch
-- @hook ENTITY:EndTouch
-- @hook ENTITY:Use
-- @hook ENTITY:OnRemove
-- @hook ENTITY:OnTakeDamage
-- @hook WEAPON:PrimaryAttack
-- @hook WEAPON:SecondaryAttack
-- @hook WEAPON:Reload
-- @hook WEAPON:Deploy
-- @hook WEAPON:Holster
-- @hook WEAPON:Think
-- @hook ENTITY:SetupDataTables
-- @hook ENTITY:KeyValue
-- @hook ENTITY:AcceptInput
-- @hook ENTITY:PreEntityCopy
-- @hook ENTITY:PostEntityCopy
-- @hook ENTITY:OnEntityCopyTableFinish
-- @hook ENTITY:PostEntityPaste
-- @hook ENTITY:OnDuplicated
-- @hook WEAPON:CanPrimaryAttack
-- @hook WEAPON:CanSecondaryAttack
-- @hook WEAPON:SetupDataTables
-- @hook WEAPON:Initialize
-- @hook WEAPON:Equip
-- @hook WEAPON:OwnerChanged
-- @hook WEAPON:Ammo1
-- @hook WEAPON:Ammo2
-- @hook WEAPON:CanBePickedUpByNPCs
-- @hook WEAPON:SetDeploySpeed
-- @hook WEAPON:SetWeaponHoldType
-- @hook WEAPON:ShootBullet
-- @hook WEAPON:ShouldDropOnDie
-- @hook WEAPON:TakePrimaryAmmo
-- @hook WEAPON:TakeSecondaryAmmo
-- Scripted entity and weapon tables in GLua form (ENT / SWEP, scripted_ents / weapons /
-- baseclass) over the host's entity.register / weapon.register and their event callbacks.
scripted_ents = {}
weapons = {}
-- weapon_base behaviour every SWEP inherits unless it overrides it
weapons.__base = {}
local WB = weapons.__base
function WB:CanPrimaryAttack() local c = rawget(self, "__clip1"); if c ~= nil and c <= 0 then return false end return true end
function WB:CanSecondaryAttack() local c = rawget(self, "__clip2"); if c ~= nil and c <= 0 then return false end return true end
function WB:TakePrimaryAmmo(n) local c = rawget(self, "__clip1") or (self.Primary and self.Primary.DefaultClip) or 0; rawset(self, "__clip1", math.max(0, c - (n or 1))) end
function WB:TakeSecondaryAmmo(n) local c = rawget(self, "__clip2") or (self.Secondary and self.Secondary.DefaultClip) or 0; rawset(self, "__clip2", math.max(0, c - (n or 1))) end
function WB:Ammo1() local o = self.Owner; if IsValid(o) and o.GetAmmoCount and self.Primary then return o:GetAmmoCount(self.Primary.Ammo) end return 0 end
function WB:Ammo2() local o = self.Owner; if IsValid(o) and o.GetAmmoCount and self.Secondary then return o:GetAmmoCount(self.Secondary.Ammo) end return 0 end
function WB:ShootBullet(dmg, num, cone, ammo, force, tracer)
    local o = self.Owner
    if not IsValid(o) then return end
    o:FireBullets({ Num = num or 1, Src = o:GetShootPos(), Dir = o:GetAimVector(), Spread = Vector(cone or 0, cone or 0, 0), Damage = dmg or 0, Force = force or 1, Attacker = o, AmmoType = ammo or (self.Primary and self.Primary.Ammo) or "" })
end
function WB:SetDeploySpeed(s) rawset(self, "__deploy_speed", s) end
function WB:SetWeaponHoldType(h) rawset(self, "__holdtype", h); self.HoldType = h end
function WB:ShouldDropOnDie() return true end
function WB:CanBePickedUpByNPCs() return false end
function WB:Equip(newOwner) end
function WB:OwnerChanged() end
function WB:Initialize() end
function WB:SetupDataTables() end
baseclass = {}
local ent_defs, ent_aliases, wep_defs, bases = {}, {}, {}, {}
local instances, by_id = {}, {}
function baseclass.Set(name, tab) bases[name] = tab end
function baseclass.Get(name) return bases[name] end
local function inherit(def, store)
    local base = def.Base
    if base and store[base] and store[base] ~= def then
        local b = inherit(store[base], store)
        local merged = table.Copy(b)
        for k, v in pairs(def) do merged[k] = v end
        merged.BaseClass = b
        return merged
    end
    return def
end
local function instance_for(store, defs, class, id, extra)
    local key = class .. ":" .. tostring(id)
    local inst = instances[key]
    if inst then return inst end
    local def = inherit(defs[class], defs)
    if extra == nil and scripted_ents.__is_nextbot and scripted_ents.__is_nextbot(defs[class]) then
        for k, v in pairs(scripted_ents.__nextbot_methods) do if def[k] == nil then def[k] = v end end
    end
    if extra == nil and scripted_ents.__is_ai and scripted_ents.__is_ai(defs[class]) then
        for k, v in pairs(scripted_ents.__ai_methods) do if def[k] == nil then def[k] = v end end
    end
    local wrapper = extra or Entity(id)
    inst = setmetatable({}, { __index = function(_, k)
        local v = def[k]
        if v ~= nil then return v end
        return wrapper[k]
    end, __newindex = function(t, k, v) rawset(t, k, v) end })
    rawset(inst, "Entity", wrapper)
    instances[key] = inst
    if extra == nil then by_id[id] = inst end
    return inst
end
function scripted_ents.Register(ENT, class)
    if hook.Run("PreRegisterSENT", ENT, class) == false then return end
    ENT.ClassName = class
    ent_defs[class] = ENT
    baseclass.Set(class, ENT)
    if ENT.Spawnable == false and ENT.Type ~= "anim" and ENT.Type ~= "brush" and ENT.Type ~= "point" then return end
    entity.register(class, {
        model = ENT.Model, think_interval = ENT.ThinkInterval, use_type = ENT.UseType,
        print_name = ENT.PrintName, category = ENT.Category,
        init = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.SetupDataTables and not rawget(i, "__dt") then rawset(i, "__dt", true); i:SetupDataTables() end if i.Initialize then i:Initialize() end if i.UpdateTransmitState then local m = i:UpdateTransmitState(); if type(m) == "number" then world.queue("ent.transmit", p.entity, m) end end end,
        think = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if scripted_ents.__is_nextbot(ENT) then scripted_ents.__nextbot_think(i) end if scripted_ents.__is_ai(ENT) and i.RunAI then i:RunAI() end if i.Think then local r = i:Think(); local nt = rawget(i, "__next_think"); if nt then rawset(i, "__next_think", nil); return nt end return r end end,
        start_touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.StartTouch then i:StartTouch(Entity(p.other)) end end,
        touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Touch then i:Touch(Entity(p.other)) end end,
        end_touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.EndTouch then i:EndTouch(Entity(p.other)) end end,
        use = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Use then i:Use(Player(p.player), Player(p.player), USE_ON, 1) end end,
        use_off = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Use then i:Use(Player(p.player), Player(p.player), USE_OFF, 0) end end,
        remove = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.OnRemove then i:OnRemove() end by_id[p.entity] = nil; instances[class .. ":" .. tostring(p.entity)] = nil end,
        damage = function(p)
            local i = instance_for(ent_defs, ent_defs, class, p.entity)
            if i.OnTakeDamage then local d = DamageInfo(); d:SetDamage(p.amount or 0); if p.player and p.player ~= 0 then d:SetAttacker(Player(p.player)) end return i:OnTakeDamage(d) end
        end,
    })
end
function scripted_ents.__instance(id) return by_id[id] end
function scripted_ents.Get(class)
    local def = ent_defs[class] or (ent_aliases[class] and ent_defs[ent_aliases[class]])
    if not def then return nil end
    return inherit(def, ent_defs)
end
function scripted_ents.GetStored(class) local d = ent_defs[class]; return d and { t = d, type = d.Type or "anim", Base = d.Base } or nil end
function scripted_ents.GetList() local o = {}; for c, d in pairs(ent_defs) do o[c] = { t = d, type = d.Type or "anim", Base = d.Base } end return o end
function scripted_ents.GetSpawnable() local o = {}; for c, d in pairs(ent_defs) do if d.Spawnable then o[c] = d end end return o end
function scripted_ents.GetType(class) local d = ent_defs[class]; return d and (d.Type or "anim") or nil end
function scripted_ents.GetMember(class, member) local d = scripted_ents.Get(class); return d and d[member] end
function scripted_ents.Alias(alias, class) ent_aliases[alias] = class end
function scripted_ents.IsBasedOn(class, base)
    local d = ent_defs[class]
    while d do if d.ClassName == base then return true end d = d.Base and ent_defs[d.Base] end
    return false
end
function scripted_ents.OnLoaded() for c, d in pairs(ent_defs) do if d.OnLoaded then d:OnLoaded() end end end
local ENTm = __glua_classes.Entity
function ENTm:GetClassName() return self:GetClass() end
local base_next = ENTm.NextThink
local SWEP_wrappers = {}
function weapons.Register(SWEP, class)
    local WB = weapons.__base
    for k, v in pairs(WB) do if SWEP[k] == nil then SWEP[k] = v end end
    if hook.Run("PreRegisterSWEP", SWEP, class) == false then return end
    SWEP.ClassName = class
    wep_defs[class] = SWEP
    baseclass.Set(class, SWEP)
    local P, S = SWEP.Primary or {}, SWEP.Secondary or {}
    local function inst(p) return instance_for(wep_defs, wep_defs, class, p.player, __glua_classes.Weapon.__from(class, p.player)) end
    weapon.register(class, {
        print_name = SWEP.PrintName, slot = SWEP.Slot, slot_pos = SWEP.SlotPos, hold_type = SWEP.HoldType,
        view_model = SWEP.ViewModel, world_model = SWEP.WorldModel, clip_size = P.ClipSize, default_clip = P.DefaultClip,
        ammo = P.Ammo, automatic = P.Automatic, primary_delay = SWEP.PrimaryDelay, secondary_delay = SWEP.SecondaryDelay, reload_time = SWEP.ReloadTime,
        primary = function(p) local i = inst(p); rawset(i, "Owner", Player(p.player)); if i.CanPrimaryAttack and i:CanPrimaryAttack() == false then return end if i.PrimaryAttack then return i:PrimaryAttack() end end,
        secondary = function(p) local i = inst(p); rawset(i, "Owner", Player(p.player)); if i.CanSecondaryAttack and i:CanSecondaryAttack() == false then return end if i.SecondaryAttack then return i:SecondaryAttack() end end,
        reload = function(p) local i = inst(p); if i.Reload then return i:Reload() end end,
        deploy = function(p) local i = inst(p); rawset(i, "Owner", Player(p.player)); if i.SetupDataTables and not rawget(i, "__dt") then rawset(i, "__dt", true); i:SetupDataTables() end if i.Initialize and not rawget(i, "__init") then rawset(i, "__init", true); i:Initialize() end if i.Deploy then return i:Deploy() end end,
        holster = function(p) local i = inst(p); if i.Holster then return i:Holster(NULL) end end,
        think = function(p) local i = inst(p); if i.Think then return i:Think() end end,
    })
end
function weapons.__ensure(class, player) if not wep_defs[class] then return nil end return instance_for(wep_defs, wep_defs, class, player, __glua_classes.Weapon.__from(class, player)) end
function weapons.Get(class) local d = wep_defs[class]; return d and inherit(d, wep_defs) or nil end
function weapons.GetStored(class) return wep_defs[class] end
function weapons.GetList() local o = {}; for _, d in pairs(wep_defs) do o[#o + 1] = d end return o end
function weapons.IsBasedOn(class, base)
    local d = wep_defs[class]
    while d do if d.ClassName == base then return true end d = d.Base and wep_defs[d.Base] end
    return false
end
function weapons.OnLoaded() for _, d in pairs(wep_defs) do if d.OnLoaded then d:OnLoaded() end end end
USE_OFF, USE_ON, USE_SET, USE_TOGGLE = 0, 1, 2, 3
SIMPLE_USE, CONTINUOUS_USE, ONOFF_USE, DIRECTIONAL_USE = 0, 1, 2, 3

-- @hook ENTITY:PhysicsCollide
-- a physics impact on a scripted entity (`ENTITY:PhysicsCollide`): the server reports the contact
-- point and approach speed; the normal is approximated from the hit point towards the entity
-- centre and the velocities are the snapshot's (old == new) - no per-contact normals yet
events.on("entity.collide", function(p)
    local i = by_id[p.entity]
    if not i or not i.PhysicsCollide then return end
    local e = Entity(p.entity)
    local other = p.other ~= 0 and Entity(p.other) or game.GetWorld()
    local hit = Vector(p.pos.x, p.pos.y, p.pos.z)
    local n = e:GetPos() - hit
    if n:Length() > 0.001 then n:Normalize() else n = Vector(0, 0, 1) end
    local ov, tv = e:GetVelocity(), other:GetVelocity()
    i:PhysicsCollide({
        HitPos = hit, HitNormal = n, HitEntity = other, HitObject = other:GetPhysicsObject(),
        PhysObject = e:GetPhysicsObject(), Speed = p.speed, HitSpeed = p.speed, DeltaTime = FrameTime(),
        OurOldVelocity = ov, OurNewVelocity = ov, TheirOldVelocity = tv, TheirNewVelocity = tv,
    }, e:GetPhysicsObject())
end)

-- @hook WEAPON:OnDrop
-- the player dropped this scripted weapon (server event with the world entity it became)
events.on("weapon.drop", function(p)
    local i = weapons.__ensure(p.class, p.player)
    if i and i.OnDrop then i:OnDrop() end
end)

-- @hook WEAPON:OnRemove
-- the weapon left the player's inventory (strip, death); the instance stays for a re-equip
events.on("weapon.remove", function(p)
    local i = weapons.__ensure(p.class, p.player)
    if i and i.OnRemove then i:OnRemove() end
end)

-- @hook NEXTBOT:RunBehaviour
-- @hook NEXTBOT:BehaveStart
-- @hook NEXTBOT:BehaveUpdate
-- NextBots: a scripted entity with Base "base_nextbot" (or Type "nextbot") gets `self.loco`, a
-- CLuaLocomotion over the server's ground mover (nextbot.* writes, state from the snapshot), and
-- RunBehaviour runs as a coroutine resumed on every think (coroutine.yield() waits a tick).
__glua_classes.CLuaLocomotion = __glua_classes.CLuaLocomotion or {}
local LOCO = __glua_classes.CLuaLocomotion
LOCO.__index = LOCO
local function loco_state(self) local sn = world and world.get(self.__id); return sn and sn.nextbot end
local function lv(self, k, d) local st = loco_state(self); local v = st and st[k]; if v == nil then return d end return v end
local function lq(self, op, v) world.queue(op, self.__id, v) end
local function v3(v) return { x = v.x, y = v.y, z = v.z } end
function LOCO:Approach(pos, goalweight) lq(self, "nextbot.approach", v3(pos)) end
function LOCO:FaceTowards(pos) lq(self, "nextbot.face", v3(pos)) end
function LOCO:SetDesiredSpeed(v) lq(self, "nextbot.speed", tonumber(v) or 0) end
function LOCO:GetDesiredSpeed() return lv(self, "desired_speed", 0) end
function LOCO:SetAcceleration(v) lq(self, "nextbot.accel", tonumber(v) or 0) end
function LOCO:GetAcceleration() return lv(self, "accel", 400) end
function LOCO:SetDeceleration(v) lq(self, "nextbot.decel", tonumber(v) or 0) end
function LOCO:GetDeceleration() return lv(self, "decel", 400) end
function LOCO:SetStepHeight(v) lq(self, "nextbot.step", tonumber(v) or 0) end
function LOCO:GetStepHeight() return lv(self, "step_height", 18) end
function LOCO:SetJumpHeight(v) lq(self, "nextbot.jump_height", tonumber(v) or 0) end
function LOCO:GetJumpHeight() return lv(self, "jump_height", 58) end
function LOCO:GetMaxJumpHeight() return self:GetJumpHeight() end
function LOCO:SetMaxYawRate(v) lq(self, "nextbot.yaw_rate", tonumber(v) or 0) end
function LOCO:GetMaxYawRate() return lv(self, "max_yaw_rate", 180) end
function LOCO:SetGravity(v) lq(self, "nextbot.gravity", tonumber(v) or 0) end
function LOCO:GetGravity() return lv(self, "gravity", 600) end
function LOCO:Jump() lq(self, "nextbot.jump", nil) end
function LOCO:JumpAcrossGap(pos, normal) lq(self, "nextbot.jump_gap", v3(pos)) end
function LOCO:ClearStuck() lq(self, "nextbot.clear_stuck", nil) end
function LOCO:SetVelocity(v) lq(self, "nextbot.velocity", v3(v)) end
function LOCO:GetVelocity() local st = loco_state(self); local v = st and st.velocity; return v and Vector(v.x, v.y, v.z) or Vector() end
function LOCO:GetGroundSpeed() return self:GetVelocity():Length2D() end
function LOCO:GetGroundMotionVector() local v = self:GetVelocity(); v.z = 0; if v:Length() > 0 then v:Normalize() end return v end
function LOCO:IsOnGround() return lv(self, "on_ground", false) end
function LOCO:IsStuck() return lv(self, "stuck", 0) >= 3 end
function LOCO:IsAttemptingToMove() return self:GetDesiredSpeed() > 0 and self:GetVelocity():Length2D() > 0.5 end
function LOCO:IsClimbingOrJumping() return not self:IsOnGround() end
function LOCO:IsAreaTraversable(area) return true end
function LOCO:GetGroundNormal() return Vector(0, 0, 1) end
function LOCO:GetDeathDropHeight() return rawget(self, "__death_drop") or 200 end
function LOCO:SetDeathDropHeight(h) rawset(self, "__death_drop", tonumber(h) or 200) end
function LOCO:GetAvoidAllowed() return rawget(self, "__avoid") ~= false end
function LOCO:SetAvoidAllowed(b) rawset(self, "__avoid", b ~= false) end
function LOCO:GetClimbAllowed() return rawget(self, "__climb") ~= false end
function LOCO:SetClimbAllowed(b) rawset(self, "__climb", b ~= false) end
function LOCO:GetJumpGapsAllowed() return rawget(self, "__gaps") ~= false end
function LOCO:SetJumpGapsAllowed(b) rawset(self, "__gaps", b ~= false) end
scripted_ents.__is_nextbot = function(ENT) return ENT and (ENT.Base == "base_nextbot" or ENT.Type == "nextbot") end
-- the coroutine behaviour: created on the first think, resumed every think until it ends
function scripted_ents.__nextbot_think(i)
    if not rawget(i, "loco") then rawset(i, "loco", setmetatable({ __id = i.Entity.__id }, LOCO)) end
    local co = rawget(i, "__behaviour")
    if co == nil and i.RunBehaviour then
        co = coroutine.create(function() i:RunBehaviour() end)
        rawset(i, "__behaviour", co)
        if i.BehaveStart then i:BehaveStart() end
    end
    if navmesh and navmesh.IsLoaded and navmesh.IsLoaded() then
        local a = navmesh.GetNavArea(i.Entity:GetPos(), 120)
        if a ~= nil and a ~= NULL then rawset(i, "__area", a) end
    end
    if i.BehaveUpdate then i:BehaveUpdate(engine.TickInterval()) end
    if co and coroutine.status(co) == "suspended" then
        local ok, err = coroutine.resume(co)
        if not ok then ErrorNoHalt(tostring(err) .. "\n") end
    end
    return 0
end
__glua_classes.NextBot = __glua_classes.NextBot or {}
local NB = __glua_classes.NextBot
function NB:MoveToPos(pos, options)
    options = options or {}
    local path = Path("Follow")
    path:SetMinLookAheadDistance(options.lookahead or 300)
    path:SetGoalTolerance(options.tolerance or 20)
    path:Compute(self, pos)
    if not path:IsValid() then if self.OnMovementFailed then self:OnMovementFailed() end return "failed" end
    while path:IsValid() do
        path:Update(self)
        if options.draw then path:Draw() end
        if self.loco:IsStuck() then
            self:HandleStuck()
            if self.OnMovementFailed then self:OnMovementFailed() end
            return "stuck"
        end
        if options.maxage and path:GetAge() > options.maxage then path:Compute(self, pos) end
        if options.repath and path:GetAge() > options.repath then path:Compute(self, pos) end
        coroutine.yield()
    end
    if self.OnMovementComplete then self:OnMovementComplete() end
    return "ok"
end
function NB:HandleStuck() self.loco:ClearStuck() end
function NB:GetRangeTo(pos) if type(pos) == "table" and pos.GetPos then pos = pos:GetPos() end return self:GetPos():Distance(pos) end
function NB:GetRangeSquaredTo(pos) local d = self:GetRangeTo(pos); return d * d end
function NB:IsNextBot() return true end
-- activity / sequence helpers over the entity animator
function NB:StartActivity(act) rawset(self, "__act", act); self:SetAnimation(act) end
function NB:GetActivity() local a = self:GetSequenceActivity(self:GetSequence()); if a == -1 then return rawget(self, "__act") or -1 end return a end
-- plays `seq` (name or index) at `speed`, yielding the behaviour coroutine until it finished
function NB:PlaySequenceAndWait(seq, speed)
    speed = tonumber(speed) or 1
    self:SetSequence(seq)
    self:SetPlaybackRate(speed)
    local dur = self:SequenceDuration(type(seq) == "number" and seq or self:LookupSequence(seq)) or 0
    local until_t = CurTime() + (speed > 0 and dur / speed or dur)
    while CurTime() < until_t and not self:IsSequenceFinished() do coroutine.yield() end
end
-- move_x / move_y pose parameters from the locomotion velocity in the body frame
function NB:BodyMoveXY()
    local v = self.loco and self.loco:GetVelocity() or Vector()
    local speed = self.loco and self.loco:GetDesiredSpeed() or 0
    if speed <= 0 then speed = math.max(v:Length2D(), 1) end
    local ang = self:GetAngles()
    local fwd, right = ang:Forward(), ang:Right()
    self:SetPoseParameter("move_x", (v.x * fwd.x + v.y * fwd.y) / speed)
    self:SetPoseParameter("move_y", (v.x * right.x + v.y * right.y) / speed)
end
function NB:SetFOV(f) rawset(self, "__fov", tonumber(f) or 90) end
function NB:GetFOV() return rawget(self, "__fov") or 90 end
function NB:SetMaxVisionRange(r) rawset(self, "__vision", tonumber(r) or 2000) end
function NB:GetMaxVisionRange() return rawget(self, "__vision") or 2000 end
-- within range, inside the FOV cone when asked, and a clear line from the eyes
function NB:IsAbleToSee(ent, useFOV)
    if not IsValid(ent) then return false end
    local eye = self:EyePos()
    local target = ent.WorldSpaceCenter and ent:WorldSpaceCenter() or ent:GetPos()
    local d = target - eye
    if d:Length() > self:GetMaxVisionRange() then return false end
    if useFOV ~= false and d:Length() > 1 then
        local cosang = math.cos(math.rad(self:GetFOV() * 0.5))
        local dn = Vector(d.x, d.y, d.z); dn:Normalize()
        if dn:Dot(self:GetAngles():Forward()) < cosang then return false end
    end
    local tr = util.TraceLine({ start = eye, endpos = target, filter = self.Entity or self })
    return not tr.Hit or tr.Fraction > 0.999 or tr.Entity == ent
end
function NB:GetLastKnownArea() return rawget(self, "__area") or NULL end
function NB:ClearLastKnownArea() rawset(self, "__area", nil) end
scripted_ents.__nextbot_methods = NB

-- @hook ENTITY:SelectSchedule
-- @hook ENTITY:RunAI
-- @hook ENTITY:StartSchedule
-- @hook ENTITY:DoSchedule
-- @hook ENTITY:SetTask
-- @hook ENTITY:NextTask
-- @hook ENTITY:StartTask
-- @hook ENTITY:RunTask
-- @hook ENTITY:TaskFinished
-- @hook ENTITY:TaskTime
-- @hook ENTITY:ScheduleFinished
-- Scripted NPC (Type "ai") schedules, as the game's base_ai runs them: RunAI every think picks
-- SelectSchedule() when idle and steps the current schedule task by task (StartTask on the
-- first think, RunTask after, TaskComplete() ends a task, NextTask / ScheduleFinished).
-- Engine tasks (EngTask) have no backend here and complete at once.
local AIM = {}
function AIM:SelectSchedule() end
function AIM:StartSchedule(sched) self.CurrentSchedule = sched; self.CurrentTaskID = 1; self:SetTask(sched:GetTask(1)) end
function AIM:SetTask(task) self.CurrentTask = task; self.TaskStatus = TASKSTATUS_NEW; self.TaskStartTime = CurTime() end
function AIM:TaskComplete() self.TaskStatus = TASKSTATUS_COMPLETE end
function AIM:TaskFinished() return self.TaskStatus == TASKSTATUS_COMPLETE end
function AIM:TaskTime() return CurTime() - (self.TaskStartTime or CurTime()) end
function AIM:StartTask(task) if task:IsEngineType() then self:TaskComplete() else task:Start(self) end end
function AIM:RunTask(task) if not task:IsEngineType() then task:Run(self) end end
function AIM:NextTask(sched)
    self.CurrentTaskID = (self.CurrentTaskID or 0) + 1
    if self.CurrentTaskID <= sched:NumTasks() then self:SetTask(sched:GetTask(self.CurrentTaskID)) else self.CurrentTask = nil end
end
function AIM:ScheduleFinished() self.CurrentSchedule = nil; self.CurrentTask = nil; self.CurrentTaskID = 0 end
function AIM:DoSchedule(sched)
    local task = self.CurrentTask
    if task then
        if self.TaskStatus == TASKSTATUS_NEW then self.TaskStatus = TASKSTATUS_RUNNING; self:StartTask(task)
        elseif self.TaskStatus == TASKSTATUS_RUNNING then self:RunTask(task) end
    end
    if self:TaskFinished() then self:NextTask(sched) end
    if (self.CurrentTaskID or 0) > sched:NumTasks() then self:ScheduleFinished() end
end
function AIM:RunAI()
    local s = self.CurrentSchedule
    if s then self:DoSchedule(s) else local n = self:SelectSchedule(); if n then self:StartSchedule(n) end end
end
scripted_ents.__is_ai = function(ENT) return ENT ~= nil and ENT.Type == "ai" end
scripted_ents.__ai_methods = AIM

-- @hook ENTITY:OnMovementComplete
-- @hook ENTITY:OnMovementFailed
-- @hook ENTITY:UpdateTransmitState
-- @hook ENTITY:PhysicsUpdate
-- PhysicsUpdate(phys) every server tick on the scripted entities that define it
events.on("tick.pre", function()
    for id, i in pairs(by_id) do
        if i.PhysicsUpdate then i:PhysicsUpdate(i.Entity:GetPhysicsObject()) end
    end
end)
