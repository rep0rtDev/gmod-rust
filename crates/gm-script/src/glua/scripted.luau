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
-- @hook ENTITY:PassesTriggerFilters
-- a scripted entity's touch callbacks only see what its PassesTriggerFilters lets through
local function passes_trigger(i, other)
    return i.PassesTriggerFilters == nil or i:PassesTriggerFilters(other) ~= false
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
        think = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if scripted_ents.__is_nextbot(ENT) then scripted_ents.__nextbot_think(i) end if scripted_ents.__is_ai(ENT) then scripted_ents.__state_change(i) if i.RunAI then i:RunAI() end end if i.Think then local r = i:Think(); local nt = rawget(i, "__next_think"); if nt then rawset(i, "__next_think", nil); return nt end return r end end,
        -- @hook NEXTBOT:OnContact
        start_touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); local o = Entity(p.other); if i.StartTouch and passes_trigger(i, o) then i:StartTouch(o) end if scripted_ents.__is_nextbot(ENT) and i.OnContact then i:OnContact(Entity(p.other)) end end,
        touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); local o = Entity(p.other); if i.Touch and passes_trigger(i, o) then i:Touch(o) end end,
        end_touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); local o = Entity(p.other); if i.EndTouch and passes_trigger(i, o) then i:EndTouch(o) end end,
        use = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Use then i:Use(Player(p.player), Player(p.player), USE_ON, 1) end end,
        use_off = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Use then i:Use(Player(p.player), Player(p.player), USE_OFF, 0) end end,
        remove = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.OnRemove then i:OnRemove() end by_id[p.entity] = nil; instances[class .. ":" .. tostring(p.entity)] = nil end,
        damage = function(p)
            local i = instance_for(ent_defs, ent_defs, class, p.entity)
            local d = DamageInfo(); d:SetDamage(p.amount or 0); if p.player and p.player ~= 0 then d:SetAttacker(Player(p.player)) end
            -- @hook NEXTBOT:OnInjured
            if scripted_ents.__is_nextbot(ENT) and i.OnInjured then i:OnInjured(d) end
            if i.OnTakeDamage then return i:OnTakeDamage(d) end
        end,
    })
end
function scripted_ents.__instance(id) return by_id[id] end
function scripted_ents.Get(class)
    if scripted_ents.__ensure_ai_goal then scripted_ents.__ensure_ai_goal(class) end
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
local wep_live = {}
-- carried scripted weapons (set on equip, cleared when the weapon leaves the inventory)
function weapons.__live(i, on) wep_live[i] = on or nil end
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
    if i then weapons.__live(i, false) end
    if i and i.OnRemove then i:OnRemove() end
end)

-- @hook WEAPON:EquipAmmo
-- the player walked into a weapon of a class he already carries (the entity is gone, the
-- inventory unchanged - measured)
events.on("player.weapon_equip_ammo", function(p)
    local i = weapons.__ensure(p.class, p.player)
    if i and i.EquipAmmo then i:EquipAmmo(Player(p.player)) end
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
-- @hook ENTITY:IsJumpLegal
-- the bot is asked whether the gap jump is legal (start, apex above the midpoint by the jump
-- height, landing); false cancels it
function LOCO:JumpAcrossGap(pos, normal)
    local e = rawget(self, "__ent")
    if e and e.IsJumpLegal and e.Entity then
        local start = e.Entity:GetPos()
        local apex = (start + pos) / 2 + Vector(0, 0, self:GetJumpHeight())
        if e:IsJumpLegal(start, apex, pos) == false then return end
    end
    lq(self, "nextbot.jump_gap", v3(pos))
end
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
    if not rawget(i, "loco") then rawset(i, "loco", setmetatable({ __id = i.Entity.__id, __ent = i }, LOCO)) end
    local co = rawget(i, "__behaviour")
    if co == nil and i.RunBehaviour then
        co = coroutine.create(function() i:RunBehaviour() end)
        rawset(i, "__behaviour", co)
        if i.BehaveStart then i:BehaveStart() end
    end
    if navmesh and navmesh.IsLoaded and navmesh.IsLoaded() then
        local a = navmesh.GetNavArea(i.Entity:GetPos(), 120)
        if a ~= nil and a ~= NULL then
            local old = rawget(i, "__area")
            rawset(i, "__area", a)
            -- @hook NEXTBOT:OnNavAreaChanged
            if old ~= nil and old ~= a and old:GetID() ~= a:GetID() and i.OnNavAreaChanged then i:OnNavAreaChanged(old, a) end
        end
    end
    -- @hook NEXTBOT:BodyUpdate
    if i.BodyUpdate then i:BodyUpdate() end
    -- sight: every entity that comes into / leaves the bot's vision cone (OnEntitySight / OnEntitySightLost)
    if i.OnEntitySight or i.OnEntitySightLost then
        local seen = rawget(i, "__seen") or {}
        local now = {}
        for _, e in ipairs(ents.GetAll()) do
            if IsValid(e) and e ~= i.Entity and (e:IsPlayer() or e:IsNPC() or e.IsNextBot and e:IsNextBot()) and i.Entity.IsAbleToSee and i.Entity:IsAbleToSee(e) then
                now[e:EntIndex()] = e
                -- @hook NEXTBOT:OnEntitySight
                if not seen[e:EntIndex()] and i.OnEntitySight then i:OnEntitySight(e) end
            end
        end
        -- @hook NEXTBOT:OnEntitySightLost
        for id, e in pairs(seen) do if not now[id] and i.OnEntitySightLost then i:OnEntitySightLost(e) end end
        rawset(i, "__seen", now)
    end
    -- @hook NEXTBOT:OnIgnite
    local burning = i.Entity.IsOnFire and i.Entity:IsOnFire() or false
    if burning and not rawget(i, "__was_burning") and i.OnIgnite then i:OnIgnite() end
    rawset(i, "__was_burning", burning)
    -- the mover's state this think against the last one: stuck / ground flips are the callbacks
    local loco = rawget(i, "loco")
    local stuck, ground = loco:IsStuck(), loco:IsOnGround()
    local ps, pg = rawget(i, "__was_stuck"), rawget(i, "__was_ground")
    rawset(i, "__was_stuck", stuck); rawset(i, "__was_ground", ground)
    -- @hook NEXTBOT:OnStuck
    -- @hook NEXTBOT:OnUnStuck
    if ps ~= nil and ps ~= stuck then
        if stuck then if i.OnStuck then i:OnStuck() end elseif i.OnUnStuck then i:OnUnStuck() end
    end
    -- @hook NEXTBOT:OnLandOnGround
    -- @hook NEXTBOT:OnLeaveGround
    if pg ~= nil and pg ~= ground then
        local g = i.Entity:GetGroundEntity()
        if ground then if i.OnLandOnGround then i:OnLandOnGround(g) end elseif i.OnLeaveGround then i:OnLeaveGround(g) end
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
    if self:IsRunningBehavior() then return true end
    if self:__EngineThink() or self:DoingEngineSchedule() then return true end
    local s = self.CurrentSchedule
    if s then self:DoSchedule(s) else local n = self:SelectSchedule(); if n then self:StartSchedule(n) end end
    self:MaintainActivity()
end
scripted_ents.__is_ai = function(ENT) return ENT ~= nil and ENT.Type == "ai" end
-- @hook ENTITY:OnCondition
-- @hook ENTITY:OnTaskComplete
-- @hook ENTITY:OnTaskFailed
-- @hook ENTITY:StartEngineSchedule
-- @hook ENTITY:EngineScheduleFinish
-- @hook ENTITY:DoingEngineSchedule
-- @hook ENTITY:StartEngineTask
-- @hook ENTITY:RunEngineTask
-- @hook ENTITY:TranslateSchedule
-- @hook ENTITY:TranslateActivity
-- @hook ENTITY:TranslateNavGoal
-- @hook ENTITY:GetRelationship
-- @hook ENTITY:GetSoundInterests
-- @hook ENTITY:GetAttackSpread
-- @hook ENTITY:OverrideMove
-- @hook ENTITY:OverrideMoveFacing
-- @hook ENTITY:OnChangeActivity
-- @hook ENTITY:OnChangeActiveWeapon
-- @hook ENTITY:ExpressionFinished
-- @hook ENTITY:SetNPCClass
-- @hook ENTITY:GetNPCClass
-- @hook ENTITY:CreateSchedulesInternal
-- @hook ENTITY:MaintainActivity
-- @hook ENTITY:IsRunningBehavior
-- The engine side of base_ai: the server brain's conditions / schedule / tasks reach the
-- scripted NPC through these callbacks every think (OnCondition on each newly set condition,
-- StartEngineSchedule when a SetSchedule() schedule begins, Start/RunEngineTask per engine task,
-- EngineScheduleFinish at its end, OnTaskComplete/Failed, activity and weapon change).
function AIM:OnCondition(cond) end
function AIM:OnTaskComplete() end
function AIM:OnTaskFailed() end
function AIM:StartEngineSchedule(sched) self:ScheduleFinished(); self.bDoingEngineSchedule = true end
function AIM:EngineScheduleFinish() self.bDoingEngineSchedule = nil end
function AIM:DoingEngineSchedule() return self.bDoingEngineSchedule end
function AIM:StartEngineTask(task, data) end
function AIM:RunEngineTask(task, data) end
function AIM:TranslateSchedule(sched) return sched end
function AIM:TranslateActivity(act) return act end
function AIM:TranslateNavGoal(goal) return goal end
function AIM:GetRelationship(ent) end
function AIM:GetSoundInterests() return 0 end
function AIM:GetAttackSpread(wep, target) return 0.1 end
function AIM:OverrideMove(interval) end
function AIM:OverrideMoveFacing(interval, data) end
function AIM:OnChangeActivity(act) end
function AIM:OnChangeActiveWeapon(old, new) end
function AIM:ExpressionFinished(name) end
function AIM:SetNPCClass(c) local e = self.Entity or self; if e then rawset(e, "__npc_class", c) end end
function AIM:GetNPCClass() local e = self.Entity or self; return (e and rawget(e, "__npc_class")) or (e and e.Classify and e:Classify()) or CLASS_NONE end
function AIM:CreateSchedulesInternal() end
function AIM:IsRunningBehavior() local e = self.Entity; return e and e.IsRunningBehavior and e:IsRunningBehavior() or false end
function AIM:MaintainActivity()
    local e = self.Entity
    if not e then return end
    local act = e.GetActivity and e:GetActivity()
    if act ~= nil and act ~= rawget(self, "__last_act") then rawset(self, "__last_act", act); self:OnChangeActivity(self:TranslateActivity(act)) end
    if e.MaintainActivity then e:MaintainActivity() end
end
-- per-think engine state diff -> callbacks; returns true while an engine schedule owns the NPC
function AIM:__EngineThink()
    local e = self.Entity
    if not (e and e.HasCondition) then return false end
    local s = world and world.get(e.__id) or nil
    if not s then return false end
    local prev = rawget(self, "__conds") or {}
    local now = {}
    for _, c in ipairs(s.npc_ai_conditions or {}) do now[c] = true; if not prev[c] then self:OnCondition(c) end end
    rawset(self, "__conds", now)
    local w = s.weapon
    if w ~= rawget(self, "__last_wep") then local old = rawget(self, "__last_wep"); rawset(self, "__last_wep", w); if old ~= nil then self:OnChangeActiveWeapon(old, w) end end
    if s.npc_ai_scripted then
        local sched = s.npc_ai_schedule
        if sched ~= rawget(self, "__eng_sched") then
            if rawget(self, "__eng_sched") ~= nil then self:EngineScheduleFinish() end
            rawset(self, "__eng_sched", sched); rawset(self, "__eng_task", nil)
            if sched ~= nil then self:StartEngineSchedule(self:TranslateSchedule(sched)) end
        end
        local task = s.npc_ai_task
        if sched ~= nil and task ~= nil then
            if task ~= rawget(self, "__eng_task") then
                if rawget(self, "__eng_task") ~= nil then self:OnTaskComplete() end
                rawset(self, "__eng_task", task); self:StartEngineTask(task, 0)
            else
                self:RunEngineTask(task, 0)
            end
        end
        if now[COND_TASK_FAILED] and not prev[COND_TASK_FAILED] then self:OnTaskFailed() end
        return sched ~= nil
    elseif rawget(self, "__eng_sched") ~= nil then
        rawset(self, "__eng_sched", nil); rawset(self, "__eng_task", nil); self:EngineScheduleFinish()
    end
    return false
end
-- @hook ENTITY:OnStateChange
-- NPC state of a scripted NPC is derived from the server AI (dead / enemy = combat / path = alert /
-- idle); the callback fires on the think where it changed
scripted_ents.__state_change = function(i)
    local e = i.Entity
    if not e then return end
    local st = __glua_classes.NPC.GetNPCState(e)
    local prev = rawget(i, "__npc_state")
    rawset(i, "__npc_state", st)
    if prev ~= nil and prev ~= st and i.OnStateChange then i:OnStateChange(prev, st) end
end
scripted_ents.__ai_methods = AIM

-- @hook ENTITY:OnMovementComplete
-- @hook ENTITY:OnMovementFailed
-- @hook ENTITY:UpdateTransmitState
-- @hook ENTITY:PhysicsUpdate
-- PhysicsUpdate(phys) every server tick on the scripted entities that define it
events.on("tick.pre", function()
    for id, i in pairs(by_id) do
        if i.PhysicsUpdate then i:PhysicsUpdate(i.Entity:GetPhysicsObject()) end
        -- @hook ENTITY:PhysicsSimulate
        -- (linear, angular, SIM_*) per tick: accelerations in u/s^2 (deg/s^2), forces over the
        -- mass; LOCAL in the entity's frame, GLOBAL in the world's; applied as velocity this tick
        if i.PhysicsSimulate then
            local phys = i.Entity:GetPhysicsObject()
            local dt = FrameTime()
            local lin, ang, mode = i:PhysicsSimulate(phys, dt)
            mode = mode or SIM_NOTHING
            if mode ~= SIM_NOTHING and IsValid(phys) then
                lin, ang = lin or Vector(), ang or Vector()
                if mode == SIM_LOCAL_FORCE or mode == SIM_GLOBAL_FORCE then
                    local m = math.max(phys:GetMass(), 1e-3)
                    lin, ang = lin / m, ang / m
                end
                if mode == SIM_LOCAL_ACCELERATION or mode == SIM_LOCAL_FORCE then
                    lin = i.Entity:LocalToWorld(lin) - i.Entity:GetPos()
                end
                phys:AddVelocity(lin * dt)
                phys:AddAngleVelocity(ang * dt)
            end
        end
    end
    -- @hook WEAPON:Tick
    -- @hook WEAPON:FreezeMovement
    -- FreezeMovement true pins the owner's walk / run / jump / crouch speeds at 0 through the
    -- per-player move overrides; false (or the weapon going away) clears them
    for i in pairs(wep_live) do
        if i.Tick then i:Tick() end
        local fr = i.FreezeMovement ~= nil and i:FreezeMovement() == true
        if fr ~= (rawget(i, "__frozen") == true) then
            rawset(i, "__frozen", fr)
            -- the carrying player: the Owner the attack path set, else the wrapper's owner id
            local o = rawget(i, "Owner")
            if o == nil and i.__owner then o = Player(i.__owner) end
            if o and o.SetWalkSpeed then
                local v = fr and 0 or nil
                o:SetWalkSpeed(v) o:SetRunSpeed(v) o:SetJumpPower(v) o:SetCrouchedWalkSpeed(v)
            end
        end
    end
end)

-- the scripted instance behind an entity index (hook bridges outside this chunk)
function scripted_ents.__instance(id) return by_id[id] end

-- The HL2 goal entities behind NPC:UseAssaultBehavior / UseLeadBehavior / UseActBusyBehavior:
-- ai_goal_assault (+ assault_rallypoint / assault_assaultpoint), ai_goal_lead, ai_goal_actbusy
-- (+ info_node_hint). Registered as point entities so ents.Create / map logic can place them;
-- Activate resolves the actor NPCs by targetname and hands the server the goal
-- (NPC:__SetAiGoal), which drives the native behaviour. Outputs come from the goal stage the
-- server reports (OnArrival / OnSuccess for a lead, OnAssaultStart etc. not raised yet).
do
    local function flag(v) return v == true or v == 1 or v == "1" or v == "true" end
    local function num(v, d) return tonumber(v) or d end
    local function kvs(e)
        local t = {}
        for k, v in pairs(e.GetKeyValues and e:GetKeyValues() or {}) do t[string.lower(k)] = v end
        local i = scripted_ents.__instance(e.__id)
        for k, v in pairs(i and i.kv or {}) do t[k] = v end
        return t
    end
    local function vec(p) return { p.x, p.y, p.z } end
    local function by_name(name)
        if not name or name == "" then return {} end
        return ents.FindByName(name)
    end
    local function actors(self)
        local out = {}
        for _, e in ipairs(by_name(kvs(self.Entity).actor)) do if e.IsNPC and e:IsNPC() then out[#out + 1] = e end end
        return out
    end
    -- registered on first use (ents.Create / scripted_ents.Get) so an addon-free runtime queues
    -- no entity registrations of its own
    local lazy = {}
    function scripted_ents.__ensure_ai_goal(class)
        local reg = lazy[class]
        if reg then lazy[class] = nil; reg() end
    end
    local function point_base(name)
        lazy[name] = function()
            local ENT = { Type = "point", Spawnable = false, Model = "" }
            function ENT:KeyValue(k, v) self.kv = self.kv or {}; self.kv[string.lower(k)] = v end
            function ENT:Initialize() self.kv = self.kv or {} end
            scripted_ents.Register(ENT, name)
        end
    end
    point_base("assault_rallypoint")
    point_base("assault_assaultpoint")
    point_base("info_node_hint")

    local function goal_base(name, behavior, build)
        lazy[name] = function() scripted_ents.__register_ai_goal(name, behavior, build) end
    end
    function scripted_ents.__register_ai_goal(name, behavior, build)
        local ENT = { Type = "point", Spawnable = false, Model = "", ThinkInterval = 0.2 }
        function ENT:KeyValue(k, v) self.kv = self.kv or {}; self.kv[string.lower(k)] = v end
        function ENT:Initialize()
            self.kv = self.kv or {}
            self.active = false
            if flag(kvs(self.Entity).startactive) then self:Start() end
        end
        function ENT:Start()
            self.active = true
            self.actors = actors(self)
            self.stages = {}
            for i, npc in ipairs(self.actors) do
                local goal = build(self, npc, i)
                if goal then
                    goal.kind = goal.kind or string.lower(behavior)
                    npc["Use" .. behavior .. "Behavior"](npc)
                    npc:__SetAiGoal(goal)
                end
            end
        end
        function ENT:Stop()
            self.active = false
            for _, npc in ipairs(self.actors or {}) do if IsValid(npc) then npc:__SetAiGoal({ kind = "clear" }) npc:UseNoBehavior() end end
        end
        function ENT:AcceptInput(input, activator, caller, data)
            local n = string.lower(input)
            if n == "activate" then self:Start() return true end
            if n == "deactivate" then self:Stop() return true end
            if n == "beginassault" and behavior == "Assault" then
                for _, npc in ipairs(self.actors or {}) do if IsValid(npc) then npc:__SetAiGoal({ kind = "begin" }) end end
                return true
            end
            if (n == "setsuccess" or n == "setfailure") and behavior == "Lead" then
                self:Stop()
                self.Entity:TriggerOutput(n == "setsuccess" and "OnSuccess" or "OnFailure", activator)
                return true
            end
            if behavior == "ActBusy" and (n == "forcenpctoactbusy" or n == "forcethisnpctoactbusy") then
                if not self.active then self:Start() end
                return true
            end
            if behavior == "ActBusy" and n == "forcethisnpctoleave" then self:Stop() return true end
            return false
        end
        function ENT:Think()
            if not self.active then return end
            for i, npc in ipairs(self.actors or {}) do
                if IsValid(npc) then
                    local st = npc:__GetAiGoalStage()
                    if st ~= self.stages[i] then
                        self.stages[i] = st
                        if behavior == "Lead" and st == 4 then
                            self.Entity:TriggerOutput("OnArrival", npc)
                            self.Entity:TriggerOutput("OnSuccess", npc)
                        elseif behavior == "Assault" and st == 4 then
                            self.Entity:TriggerOutput("OnAssaultEnd", npc)
                        end
                    end
                end
            end
        end
        scripted_ents.Register(ENT, name)
    end

    -- the rally point per actor (priority order, or random), its assault point chain
    goal_base("ai_goal_assault", "Assault", function(self, npc, i)
        local kv = kvs(self.Entity)
        local rallies = by_name(kv.rallypoint)
        table.sort(rallies, function(a, b) return num(kvs(a).priority, 0) < num(kvs(b).priority, 0) end)
        if kv.rallyselectmethod == "1" and #rallies > 0 then
            local r = rallies[math.random(#rallies)]
            table.remove(rallies, i) rallies = { r }
        end
        local rally = rallies[((i - 1) % math.max(#rallies, 1)) + 1]
        if not rally then return nil end
        local rk = kvs(rally)
        local points, seen, p = {}, {}, by_name(rk.assaultpoint)[1]
        while p and not seen[p] and #points < 32 do
            seen[p] = true
            local pk = kvs(p)
            points[#points + 1] = {
                pos = vec(p:GetPos()), tolerance = num(pk.assaulttolerance, 36), timeout = num(pk.assaulttimeout, 0),
                clear_on_contact = flag(pk.clearoncontact), force_crouch = flag(pk.forcecrouch), urgent = flag(pk.urgent),
            }
            p = by_name(pk.nextassaultpoint)[1]
        end
        -- AssaultCue: 1 entity input (BeginAssault), 2 gunfire (treated as the input), 3 none
        return {
            rally = vec(rally:GetPos()), rally_yaw = rally:GetAngles().y, rally_sequence = rk.rallysequence ~= "" and rk.rallysequence or nil,
            delay = num(rk.assaultdelay, 0), cue = kv.assaultcue == "3", points = points,
        }
    end)

    goal_base("ai_goal_lead", "Lead", function(self)
        local kv = kvs(self.Entity)
        local goal = by_name(kv.goal)[1]
        if not goal then return nil end
        return {
            goal = vec(goal:GetPos()), lead_distance = num(kv.leaddistance, 64), retrieve_distance = num(kv.retrievedistance, 128),
            success_distance = num(kv.successdistance, 32), run = flag(kv.run),
        }
    end)

    -- hint nodes of the act-busy type (12), their activity as the busy sequence
    goal_base("ai_goal_actbusy", "ActBusy", function(self)
        local kv = kvs(self.Entity)
        local hints = {}
        for _, h in ipairs(ents.FindByClass("info_node_hint")) do
            local hk = kvs(h)
            if hk.hinttype == nil or hk.hinttype == "12" or hk.hinttype == "" then
                hints[#hints + 1] = { pos = vec(h:GetPos()), sequence = hk.hintactivity ~= "" and hk.hintactivity or nil }
            end
        end
        return { hints = hints, range = num(kv.busysearchrange, 0), busy_time = num(kv.busytime, 0) }
    end)
end

