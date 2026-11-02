-- Scripted entity and weapon tables in GLua form (ENT / SWEP, scripted_ents / weapons /
-- baseclass) over the host's entity.register / weapon.register and their event callbacks.
scripted_ents = {}
weapons = {}
baseclass = {}
local ent_defs, ent_aliases, wep_defs, bases = {}, {}, {}, {}
local instances = {}
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
    local wrapper = extra or Entity(id)
    inst = setmetatable({}, { __index = function(_, k)
        local v = def[k]
        if v ~= nil then return v end
        return wrapper[k]
    end, __newindex = function(t, k, v) rawset(t, k, v) end })
    rawset(inst, "Entity", wrapper)
    instances[key] = inst
    return inst
end
function scripted_ents.Register(ENT, class)
    ENT.ClassName = class
    ent_defs[class] = ENT
    baseclass.Set(class, ENT)
    if ENT.Spawnable == false and ENT.Type ~= "anim" and ENT.Type ~= "brush" and ENT.Type ~= "point" then return end
    entity.register(class, {
        model = ENT.Model, think_interval = ENT.ThinkInterval, use_type = ENT.UseType,
        print_name = ENT.PrintName, category = ENT.Category,
        init = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Initialize then i:Initialize() end if i.SpawnFunction == nil then end end,
        think = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Think then local r = i:Think(); local nt = rawget(i, "__next_think"); if nt then rawset(i, "__next_think", nil); return nt end return r end end,
        start_touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.StartTouch then i:StartTouch(Entity(p.other)) end end,
        touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Touch then i:Touch(Entity(p.other)) end end,
        end_touch = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.EndTouch then i:EndTouch(Entity(p.other)) end end,
        use = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Use then i:Use(Player(p.player), Player(p.player), USE_ON, 1) end end,
        use_off = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.Use then i:Use(Player(p.player), Player(p.player), USE_OFF, 0) end end,
        remove = function(p) local i = instance_for(ent_defs, ent_defs, class, p.entity); if i.OnRemove then i:OnRemove() end instances[class .. ":" .. tostring(p.entity)] = nil end,
        damage = function(p)
            local i = instance_for(ent_defs, ent_defs, class, p.entity)
            if i.OnTakeDamage then local d = DamageInfo(); d:SetDamage(p.amount or 0); if p.player and p.player ~= 0 then d:SetAttacker(Player(p.player)) end return i:OnTakeDamage(d) end
        end,
    })
end
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
    SWEP.ClassName = class
    wep_defs[class] = SWEP
    baseclass.Set(class, SWEP)
    local P, S = SWEP.Primary or {}, SWEP.Secondary or {}
    local function inst(p) return instance_for(wep_defs, wep_defs, class, p.player, __glua_classes.Weapon.__from(class, p.player)) end
    weapon.register(class, {
        print_name = SWEP.PrintName, slot = SWEP.Slot, slot_pos = SWEP.SlotPos, hold_type = SWEP.HoldType,
        view_model = SWEP.ViewModel, world_model = SWEP.WorldModel, clip_size = P.ClipSize, default_clip = P.DefaultClip,
        ammo = P.Ammo, automatic = P.Automatic, primary_delay = SWEP.PrimaryDelay, secondary_delay = SWEP.SecondaryDelay, reload_time = SWEP.ReloadTime,
        primary = function(p) local i = inst(p); rawset(i, "Owner", Player(p.player)); if i.PrimaryAttack then return i:PrimaryAttack() end end,
        secondary = function(p) local i = inst(p); rawset(i, "Owner", Player(p.player)); if i.SecondaryAttack then return i:SecondaryAttack() end end,
        reload = function(p) local i = inst(p); if i.Reload then return i:Reload() end end,
        deploy = function(p) local i = inst(p); rawset(i, "Owner", Player(p.player)); if i.Deploy then return i:Deploy() end end,
        holster = function(p) local i = inst(p); if i.Holster then return i:Holster(NULL) end end,
        think = function(p) local i = inst(p); if i.Think then return i:Think() end end,
    })
end
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
