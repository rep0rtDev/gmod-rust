-- Gamemode hook bridges: the base gamemode's player animation chain, death notices, pickup
-- history, target id, menu / scoreboard / chat key hooks, the client frame hooks and the
-- server bridges that raise the remaining GM / SANDBOX / PLAYER / WEAPON / TOOL / ENTITY
-- callbacks. Loaded last so it replaces the stub defaults.
GM = GM or {}
GAMEMODE = GAMEMODE or GM
-- a bridge that fails must not take the frame / tick chain down with it: report once, go on
local reported = {}
local function safe(fn)
    return function(...)
        local ok, err = pcall(fn, ...)
        if not ok and not reported[tostring(err)] then reported[tostring(err)] = true; (ErrorNoHalt or print)("hooks: " .. tostring(err) .. "\n") end
    end
end
local function run_class(ply, fn, ...)
    if player_manager and player_manager.RunClass then return player_manager.RunClass(ply, fn, ...) end
end

------------------------------------------------------------------------ player animations (base gamemode animations.lua)
function GM:HandlePlayerJumping(ply, velocity, t)
    t = t or ply:GetTable()
    if ply:GetMoveType() == MOVETYPE_NOCLIP then t.m_bJumping = false return end
    if not t.m_bJumping and not ply:OnGround() and ply:WaterLevel() <= 0 then
        if not t.m_fGroundTime then t.m_fGroundTime = CurTime()
        elseif (CurTime() - t.m_fGroundTime) > 0 and velocity:Length2DSqr() < 0.25 then
            t.m_bJumping = true; t.m_bFirstJumpFrame = false; t.m_flJumpStartTime = 0
        end
    end
    if t.m_bJumping then
        if t.m_bFirstJumpFrame then t.m_bFirstJumpFrame = false; ply:AnimRestartMainSequence() end
        if ply:WaterLevel() >= 2 or ((CurTime() - t.m_flJumpStartTime) > 0.2 and ply:OnGround()) then
            t.m_bJumping = false; t.m_fGroundTime = nil; ply:AnimRestartMainSequence()
        end
        if t.m_bJumping then t.CalcIdeal = ACT_MP_JUMP return true end
    end
    return false
end
function GM:HandlePlayerDucking(ply, velocity, t)
    t = t or ply:GetTable()
    if not ply:IsFlagSet(FL_ANIMDUCKING) and not ply:Crouching() then return false end
    t.CalcIdeal = velocity:Length2DSqr() > 0.25 and ACT_MP_CROUCHWALK or ACT_MP_CROUCH_IDLE
    return true
end
function GM:HandlePlayerNoClipping(ply, velocity, t)
    t = t or ply:GetTable()
    if ply:GetMoveType() ~= MOVETYPE_NOCLIP or ply:InVehicle() then
        if t.m_bWasNoclipping then t.m_bWasNoclipping = nil; ply:AnimResetGestureSlot(GESTURE_SLOT_CUSTOM); if CLIENT then ply:SetIK(true) end end
        return
    end
    if not t.m_bWasNoclipping then ply:AnimRestartGesture(GESTURE_SLOT_CUSTOM, ACT_GMOD_NOCLIP_LAYER, false); if CLIENT then ply:SetIK(false) end end
    return true
end
function GM:HandlePlayerVaulting(ply, velocity, t)
    t = t or ply:GetTable()
    if velocity:LengthSqr() < 1000000 or ply:IsOnGround() then return end
    t.CalcIdeal = ACT_MP_SWIM
    return true
end
function GM:HandlePlayerSwimming(ply, velocity, t)
    t = t or ply:GetTable()
    if ply:WaterLevel() < 2 or ply:IsOnGround() then t.m_bInSwim = false return false end
    t.CalcIdeal = ACT_MP_SWIM; t.m_bInSwim = true
    return true
end
function GM:HandlePlayerLanding(ply, velocity, wasOnGround)
    if ply:GetMoveType() == MOVETYPE_NOCLIP then return end
    if ply:IsOnGround() and not wasOnGround then ply:AnimRestartGesture(GESTURE_SLOT_JUMP, ACT_LAND, true) end
end
function GM:HandlePlayerDriving(ply, t)
    t = t or ply:GetTable()
    if not ply:InVehicle() then return false end
    local veh = ply:GetVehicle()
    if not IsValid(veh) then return false end
    if not veh.HandleAnimation and veh.GetVehicleClass and list then
        local entry = list.GetEntry("Vehicles", veh:GetVehicleClass())
        if entry and entry.Members and entry.Members.HandleAnimation then veh.HandleAnimation = entry.Members.HandleAnimation else veh.HandleAnimation = true end
    end
    if type(veh.HandleAnimation) == "function" then local seq = veh:HandleAnimation(ply); if seq ~= nil then t.CalcSeqOverride = seq end end
    if t.CalcSeqOverride == -1 then
        local class = veh:GetClass()
        if class == "prop_vehicle_jeep" then t.CalcSeqOverride = ply:LookupSequence("drive_jeep")
        elseif class == "prop_vehicle_airboat" then t.CalcSeqOverride = ply:LookupSequence("drive_airboat")
        elseif class == "prop_vehicle_prisoner_pod" and veh:GetModel() == "models/vehicles/prisoner_pod_inner.mdl" then t.CalcSeqOverride = ply:LookupSequence("drive_pd")
        else t.CalcSeqOverride = ply:LookupSequence("sit_rollercoaster") end
    end
    local use_anims = t.CalcSeqOverride == ply:LookupSequence("sit_rollercoaster") or t.CalcSeqOverride == ply:LookupSequence("sit")
    if use_anims and ply:GetAllowWeaponsInVehicle() and IsValid(ply:GetActiveWeapon()) then
        local holdtype = ply:GetActiveWeapon():GetHoldType()
        if holdtype == "smg" then holdtype = "smg1" end
        local seqid = ply:LookupSequence("sit_" .. holdtype)
        if seqid ~= -1 then t.CalcSeqOverride = seqid end
    end
    return true
end
function GM:UpdateAnimation(ply, velocity, maxseqgroundspeed)
    local len = velocity:Length()
    local movement = 1.0
    if len > 0.2 then movement = len / math.max(maxseqgroundspeed or 1, 1) end
    local rate = math.min(movement, 2)
    if ply:WaterLevel() >= 2 then rate = math.max(rate, 0.5) elseif not ply:IsOnGround() and len >= 1000 then rate = 0.1 end
    local t = ply:GetTable()
    if t.__anim_rate ~= rate then t.__anim_rate = rate; ply:SetPlaybackRate(rate) end
    if ply:InVehicle() then
        local veh = ply:GetVehicle()
        if IsValid(veh) then
            local vel = veh:GetVelocity(); local fwd = veh:GetUp(); local dp = fwd:Dot(Vector(0, 0, 1))
            ply:SetPoseParameter("vertical_velocity", (dp < 0 and dp or 0) + fwd:Dot(vel) * 0.005)
            local steer = veh:GetPoseParameter("vehicle_steer") or 0
            if veh:GetClass() == "prop_vehicle_prisoner_pod" then
                steer = 0.5
                ply:SetPoseParameter("aim_yaw", math.NormalizeAngle(ply:GetAimVector():Angle().y - veh:GetAngles().y - 90))
            end
            if CLIENT then steer = steer * 2 - 1 end
            ply:SetPoseParameter("vehicle_steer", steer)
        end
    end
    hook.Run("GrabEarAnimation", ply)
    if CLIENT then hook.Run("MouthMoveAnimation", ply) end
end
function GM:GrabEarAnimation(ply, t)
    t = t or ply:GetTable()
    t.ChatGestureWeight = t.ChatGestureWeight or 0
    if ply:IsPlayingTaunt() then return end
    t.ChatGestureWeight = math.Approach(t.ChatGestureWeight, ply:IsTyping() and 1 or 0, FrameTime() * 5.0)
    if t.ChatGestureWeight > 0 then
        ply:AnimRestartGesture(GESTURE_SLOT_VCD, ACT_GMOD_IN_CHAT, true)
        ply:AnimSetGestureWeight(GESTURE_SLOT_VCD, t.ChatGestureWeight)
    end
end
function GM:MouthMoveAnimation(ply)
    local weight = ply:IsSpeaking() and math.Clamp(ply:VoiceVolume() * 2, 0, 2) or 0
    for _, name in ipairs({ "jaw_drop", "left_part", "right_part", "left_mouth_drop", "right_mouth_drop" }) do
        local id = ply:GetFlexIDByName(name)
        if id then ply:SetFlexWeight(id, weight) end
    end
end
function GM:CalcMainActivity(ply, velocity)
    local t = ply:GetTable()
    t.CalcIdeal = ACT_MP_STAND_IDLE
    t.CalcSeqOverride = -1
    hook.Run("HandlePlayerLanding", ply, velocity, t.m_bWasOnGround)
    if not (hook.Run("HandlePlayerNoClipping", ply, velocity, t) or hook.Run("HandlePlayerDriving", ply, t)
        or hook.Run("HandlePlayerVaulting", ply, velocity, t) or hook.Run("HandlePlayerJumping", ply, velocity, t)
        or hook.Run("HandlePlayerSwimming", ply, velocity, t) or hook.Run("HandlePlayerDucking", ply, velocity, t)) then
        local len2d = velocity:Length2DSqr()
        if len2d > 22500 then t.CalcIdeal = ACT_MP_RUN elseif len2d > 0.25 then t.CalcIdeal = ACT_MP_WALK end
    end
    t.m_bWasOnGround = ply:IsOnGround()
    t.m_bWasNoclipping = ply:GetMoveType() == MOVETYPE_NOCLIP and not ply:InVehicle()
    return t.CalcIdeal, t.CalcSeqOverride
end
local IdleActivity = ACT_HL2MP_IDLE or 0
local IdleActivityTranslate = {
    [ACT_MP_STAND_IDLE] = IdleActivity, [ACT_MP_WALK] = IdleActivity + 1, [ACT_MP_RUN] = IdleActivity + 2,
    [ACT_MP_CROUCH_IDLE] = IdleActivity + 3, [ACT_MP_CROUCHWALK] = IdleActivity + 4,
    [ACT_MP_ATTACK_STAND_PRIMARYFIRE] = IdleActivity + 5, [ACT_MP_ATTACK_CROUCH_PRIMARYFIRE] = IdleActivity + 5,
    [ACT_MP_RELOAD_STAND] = IdleActivity + 6, [ACT_MP_RELOAD_CROUCH] = IdleActivity + 6,
    [ACT_MP_JUMP] = ACT_HL2MP_JUMP_SLAM, [ACT_MP_SWIM] = IdleActivity + 9, [ACT_LAND] = ACT_LAND,
}
function GM:TranslateActivity(ply, act)
    local newact = ply:TranslateWeaponActivity(act)
    -- a scripted weapon may retranslate the player's activity
    -- @hook WEAPON:TranslateActivity
    local wep = ply.GetActiveWeapon and ply:GetActiveWeapon()
    if IsValid(wep) and wep.TranslateActivity then
        local t = wep:TranslateActivity(act)
        if type(t) == "number" and t ~= -1 then newact = t end
    end
    if act == newact then return IdleActivityTranslate[act] end
    return newact
end
function GM:DoAnimationEvent(ply, event, data)
    if event == PLAYERANIMEVENT_ATTACK_PRIMARY then
        ply:AnimRestartGesture(GESTURE_SLOT_ATTACK_AND_RELOAD, ply:IsFlagSet(FL_ANIMDUCKING) and ACT_MP_ATTACK_CROUCH_PRIMARYFIRE or ACT_MP_ATTACK_STAND_PRIMARYFIRE, true)
        return ACT_VM_PRIMARYATTACK
    elseif event == PLAYERANIMEVENT_ATTACK_SECONDARY then return ACT_VM_SECONDARYATTACK
    elseif event == PLAYERANIMEVENT_RELOAD then
        ply:AnimRestartGesture(GESTURE_SLOT_ATTACK_AND_RELOAD, ply:IsFlagSet(FL_ANIMDUCKING) and ACT_MP_RELOAD_CROUCH or ACT_MP_RELOAD_STAND, true)
        return ACT_INVALID
    elseif event == PLAYERANIMEVENT_JUMP then
        ply.m_bJumping = true; ply.m_bFirstJumpFrame = true; ply.m_flJumpStartTime = CurTime()
        ply:AnimRestartMainSequence()
        return ACT_INVALID
    elseif event == PLAYERANIMEVENT_CANCEL_RELOAD then ply:AnimResetGestureSlot(GESTURE_SLOT_ATTACK_AND_RELOAD) return ACT_INVALID end
end
function GM:PlayerFireAnimationEvent(ply, event) return false end
function GM:PlayerHandleAnimEvent(ply, event, eventTime, cycle, type, options) return nil end
-- the animation chain runs for every player each tick (the engine's player think)
local animate
animate = safe(function(ply)
    if not IsValid(ply) or not ply:Alive() then return end
    local t = ply:GetTable()
    if not t.__anim_init then t.__anim_init = true; t.m_bWasOnGround = ply:IsOnGround(); t.__anim_rate = 1 end
    local vel = ply:GetVelocity()
    local act, seq = hook.Run("CalcMainActivity", ply, vel)
    if act ~= nil and act ~= ACT_INVALID then
        local t = ply:GetTable()
        local tr = hook.Run("TranslateActivity", ply, act)
        if seq and seq ~= -1 then t.__anim_seq = seq elseif tr then t.__anim_act = tr end
    end
    hook.Run("UpdateAnimation", ply, vel, ply.GetSequenceGroundSpeed and ply:GetSequenceGroundSpeed(ply:GetSequence()) or 200)
end)
-- the hands model: the gamemode asks PlayerSetHandsModel, the default follows the player model
function GM:PlayerSetHandsModel(ply, hands)
    local info = player_manager and player_manager.TranslatePlayerHands and player_manager.TranslatePlayerHands(player_manager.TranslateToPlayerModelName(ply:GetModel()))
    if info and IsValid(hands) then hands:SetModel(info.model); hands:SetSkin(info.skin or 0); hands:SetBodyGroups(info.body or "0000000") end
end
function GM:SetPlayerSpeed(ply, walk, run) if ply:GetWalkSpeed() ~= walk then ply:SetWalkSpeed(walk) end if ply:GetRunSpeed() ~= run then ply:SetRunSpeed(run) end end
function GM:PlayerSpawnAsSpectator(ply) ply:StripWeapons(); if ply:Team() == TEAM_UNASSIGNED then ply:Spectate(OBS_MODE_FIXED) return end ply:SetTeam(TEAM_SPECTATOR); ply:Spectate(OBS_MODE_ROAMING) end
function GM:PlayerDeathSound(ply) return false end
function GM:PlayerSilentDeath() end
function GM:CreateEntityRagdoll(owner, ragdoll) end
function GM:CreateClientsideRagdoll(owner, ragdoll) end
function GM:PlayerCanPickupItem(ply, item) return true end
function GM:PlayerSpray(ply) return false end
function GM:PlayerShouldTaunt(ply, act) return true end
function GM:PlayerStartTaunt(ply, act, length) end
function GM:PlayerCanSeePlayersChat(text, teamOnly, listener, speaker) if teamOnly and IsValid(speaker) and IsValid(listener) and speaker:Team() ~= listener:Team() then return false end return true end
function GM:PlayerCanHearPlayersVoice(listener, talker) return true, false end
function GM:PlayerStartVoice(ply) end
function GM:PlayerEndVoice(ply) end
function GM:PlayerRequestTeam(ply, teamid)
    if not team.Joinable(teamid) then ply:ChatPrint("You can't join that team") return end
    if not hook.Run("PlayerCanJoinTeam", ply, teamid) then return end
    hook.Run("PlayerJoinTeam", ply, teamid)
end
function GM:PlayerClassChanged(ply, newID) end
function GM:CheckPassword(steamid, ip, svpass, clpass, name) if svpass == nil or svpass == "" or svpass == clpass then return true end return false, "#GameUI_ServerRejectBadPassword" end
function GM:ClientSignOnStateChanged(userid, old, new) end
function GM:GameContentChanged() end
function GM:NetworkEntityCreated(ent) end
function GM:NotifyShouldTransmit(ent, should) end
function GM:OnAchievementAchieved(ply, id) chat.AddText(ply:Nick() .. " earned the achievement " .. (achievements and achievements.GetName and achievements.GetName(id) or tostring(id))) end
function GM:OnPermissionsChanged() end
function GM:OnCrazyPhysics(ent, phys) end
function GM:GravGunPunt(ply, ent) return true end
function GM:ShouldCollide(a, b) return true end
function GM:PreCleanupMap() end
function GM:PostCleanupMap() end
function GM:StartCommand(ply, cmd) end
function GM:VehicleMove(ply, veh, mv) end
function GM:SetupPlayerVisibility(ply, viewent) if ply:InVehicle() then AddOriginToPVS(ply:GetVehicle():GetPos()) end end
function GM:GetTeamColor(ent) local t = ent:IsPlayer() and ent:Team() or TEAM_UNASSIGNED; return team.GetColor(t) end
function GM:GetTeamNumColor(num) return team.GetColor(num) end
function GM:ShowHelp(ply) end
function GM:ShowTeam(ply) end
function GM:HideTeam() end
function GM:ShowSpare1(ply) end
function GM:ShowSpare2(ply) end
function GM:PlayerDriveAnimate(ply) end
function GM:StartEntityDriving(ent, ply) if drive and drive.Start then drive.Start(ply, ent) end end
function GM:EndEntityDriving(ent, ply) if drive and drive.End then drive.End(ply, ent) end end
-- @hook ENTITY:CanEditVariables
function GM:VariableEdited(ent, ply, key, val, editor) if hook.Run("CanEditVariable", ent, ply, key, val, editor) and (not ent.CanEditVariables or ent:CanEditVariables(ply)) then ent:SetNetworkKeyValue(key, val) end end

------------------------------------------------------------------------ server bridges
if SERVER then
    local driving_last = {}
    events.on("tick.pre", safe(function()
        for _, ply in ipairs(player.GetAll()) do
            animate(ply)
            hook.Run("SetupPlayerVisibility", ply, ply:GetViewEntity())
            local d = ply:GetDrivingEntity()
            local was = driving_last[ply:UserID()]
            if IsValid(d) then
                if was ~= d then hook.Run("StartEntityDriving", d, ply) end
                hook.Run("PlayerDriveAnimate", ply)
            elseif IsValid(was) then hook.Run("EndEntityDriving", was, ply) end
            driving_last[ply:UserID()] = IsValid(d) and d or nil
            -- scripted tools: Deploy / Holster when the toolgun comes and goes, Think while held
            local t = ply:GetTable()
            local tool = ply.GetTool and ply:GetTool()
            if tool ~= t.__last_tool then
                -- gmod_tool Think (toolgun.md F17-F18, gap-report #92): a mode change on the
                -- toolgun is Holster(true) / Deploy(true) (the flag marks a tool switch);
                -- putting the toolgun itself away is a plain Holster()
                local switch = tool ~= nil and t.__last_tool ~= nil
                local was = t.__last_tool
                if was then
                    if was.ReleaseGhostEntity then was:ReleaseGhostEntity() end
                    if was.Holster then was:Holster(switch or nil) end -- @hook TOOL:Holster
                end
                if tool and tool.Deploy then tool:Deploy(switch or nil) end -- @hook TOOL:Deploy
                t.__last_tool = tool
            end
            if tool then
                if tool.CheckObjects then tool:CheckObjects() end
                if tool.Think then tool:Think() end -- @hook TOOL:Think
            end
        end
    end))
    events.on("player.spawned", safe(function(p)
        local ply = Player(p.player)
        if not IsValid(ply) then return end
        if ply:Team() == TEAM_SPECTATOR then hook.Run("PlayerSpawnAsSpectator", ply) return end
        if not ply:GetTable().__signed_on then ply:GetTable().__signed_on = true; hook.Run("ClientSignOnStateChanged", ply:UserID(), 5, 6) end
        hook.Run("PlayerSetHandsModel", ply, ply:GetHands())
        hook.Run("SetPlayerSpeed", ply, ply:GetWalkSpeed(), ply:GetRunSpeed())
    end))
    events.on("player.death", safe(function(p)
        local ply = Player(p.player)
        local attacker = p.attacker ~= 0 and Player(p.attacker) or NULL
        hook.Run("PlayerDeathSound", ply)
        local a = IsValid(attacker) and attacker or ply
        hook.Run("SendDeathNotice", a, ply, 0)
        timer.Simple(0, function() local r = IsValid(ply) and ply:GetRagdollEntity(); if IsValid(r) then hook.Run("CreateEntityRagdoll", ply, r) end end)
    end))
    function GM:SendDeathNotice(attacker, victim, flags)
        util.AddNetworkString("__gm_deathnotice")
        net.Start("__gm_deathnotice")
        -- @hook GM:GetDeathNoticeEntityName
        net.WriteString(IsValid(attacker) and attacker:IsPlayer() and attacker:Nick() or (IsValid(attacker) and hook.Run("GetDeathNoticeEntityName", attacker) or ""))
        net.WriteInt(IsValid(attacker) and attacker:IsPlayer() and attacker:Team() or -1, 16)
        net.WriteString(IsValid(attacker) and attacker:IsPlayer() and (IsValid(attacker:GetActiveWeapon()) and attacker:GetActiveWeapon():GetClass() or "") or "")
        net.WriteString(IsValid(victim) and victim:IsPlayer() and victim:Nick() or (IsValid(victim) and victim:GetClass() or ""))
        net.WriteInt(IsValid(victim) and victim:IsPlayer() and victim:Team() or -1, 16)
        net.Broadcast()
    end
    local function pickup_msg(ply, kind, a, b)
        if not IsValid(ply) then return end
        util.AddNetworkString("__gm_pickup")
        net.Start("__gm_pickup"); net.WriteString(kind); net.WriteString(tostring(a or "")); net.WriteInt(tonumber(b) or 0, 32); net.Send(ply)
    end
    events.on("player.pickup_request", safe(function(p)
        local ply, e = Player(p.player), Entity(p.entity)
        if IsValid(e) and e.GravGunPickupAllowed and e:GravGunPickupAllowed(ply) == false then return false end -- @hook ENTITY:GravGunPickupAllowed
        if IsValid(e) and string.sub(e:GetClass(), 1, 5) == "item_" and hook.Run("PlayerCanPickupItem", ply, e) == false then return false end
    end))
    events.on("player.pickup", function(p) pickup_msg(Player(p.player), "item", p.class or p.entity) end)
    events.on("player.weapon_pickup", function(p) pickup_msg(Player(p.player), "weapon", p.class or p.weapon) end)
    events.on("player.weapon_equip_ammo", function(p) pickup_msg(Player(p.player), "ammo", p.ammo or p.kind, p.count or p.amount) end)
    events.on("player.setup_move", safe(function(p)
        local ply = Player(p.player)
        local cmd = ply:GetCurrentCommand()
        hook.Run("StartCommand", ply, cmd)
        if cmd and cmd.GetImpulse and cmd:GetImpulse() == 201 and not ply:GetTable().__sprayed then
            ply:GetTable().__sprayed = CurTime()
            if hook.Run("PlayerSpray", ply) ~= true then ply:SprayDecal() end
            timer.Simple(1, function() if IsValid(ply) then ply:GetTable().__sprayed = nil end end)
        end
        -- the tool blocks movement while holding (TOOL:FreezeMovement) and the weapon's own hook
        local tool = ply.GetTool and ply:GetTool()
        local wep = ply:GetActiveWeapon()
        if IsValid(wep) and wep.FreezeMovement and wep:FreezeMovement() then return false end
        if tool and tool.FreezeMovement and tool:FreezeMovement() then return false end -- @hook TOOL:FreezeMovement
    end))
    events.on("player.move", safe(function(p)
        local ply = Player(p.player)
        if ply:InVehicle() and IsValid(ply:GetVehicle()) then hook.Run("VehicleMove", ply, ply:GetVehicle(), ply.GetMoveData and ply:GetMoveData() or nil) end
    end))
    events.on("player.command", safe(function(p)
        local ply = Player(p.player)
        local cmd = p.cmd
        if cmd == "gm_showhelp" then hook.Run("ShowHelp", ply) return "1"
        elseif cmd == "gm_showteam" then hook.Run("ShowTeam", ply) return "1"
        elseif cmd == "gm_showspare1" then hook.Run("ShowSpare1", ply) return "1"
        elseif cmd == "gm_showspare2" then hook.Run("ShowSpare2", ply) return "1"
        elseif cmd == "changeteam" then hook.Run("PlayerRequestTeam", ply, tonumber(p.args) or 0) return "1"
        elseif cmd == "act" then
            local act = p.args or ""
            if hook.Run("PlayerShouldTaunt", ply, act) ~= false then
                ply:GetTable().__taunt = true
                hook.Run("PlayerStartTaunt", ply, act, 2)
                timer.Simple(2, function() if IsValid(ply) then ply:GetTable().__taunt = false end end)
            end
            return "1"
        elseif cmd == "gm_hideteam" then hook.Run("HideTeam") return "1" end
    end))
    events.on("sandbox.cleanup", function(p) hook.Run("PreCleanupMap") end)
    events.on("sandbox.cleanup", function(p) timer.Simple(0, function() hook.Run("PostCleanupMap") end) end)
    events.on("game.save", function(p) hook.Run("PersistenceSave", p.name or "") end)
    events.on("game.restore", function(p) hook.Run("PersistenceLoad", p.name or "") end)
    events.on("entity.collide", safe(function(p)
        local a, b = Entity(p.entity or p.ent or 0), Entity(p.other or 0)
        if not IsValid(a) or not IsValid(b) then return end
        local ca = rawget(a, "__vars"); local cb = rawget(b, "__vars")
        if (ca and ca.__custom_collide) or (cb and cb.__custom_collide) then
            if hook.Run("ShouldCollide", a, b) == false then constraint.NoCollide(a, b, 0, 0) end
        end
        local speed = (p.speed or 0)
        if speed > 10000 then hook.Run("OnCrazyPhysics", a, a:GetPhysicsObject()) end
    end))
    events.on("addon.reloaded", safe(function()
        hook.Run("GameContentChanged")
        for _, e in ipairs(ents.GetAll()) do if e.OnReloaded then e:OnReloaded() end end -- @hook ENTITY:OnReloaded
    end))
    -- an animation event the server's sequence playback passed: new-style events are named
    -- and resolved through the game's event list (util.GetAnimEventIDByName)
    local function anim_event_id(p)
        if bit.band(p.type or 0, 1024) ~= 0 and p.name and p.name ~= "" then
            local id = util.GetAnimEventIDByName(p.name)
            if id and id >= 0 then return id end
        end
        return p.event or 0
    end
    -- @hook ENTITY:HandleAnimEvent
    events.on("entity.anim_event", safe(function(p)
        local e = Entity(p.entity or 0)
        if IsValid(e) and e.HandleAnimEvent then e:HandleAnimEvent(anim_event_id(p), p.time or 0, p.cycle or 0, p.type or 0, p.options or "") end
    end))
    -- @hook GM:PlayerHandleAnimEvent
    events.on("player.anim_event", safe(function(p)
        local ply = Player(p.entity or 0)
        if IsValid(ply) then
            hook.Run("PlayerFireAnimationEvent", ply, anim_event_id(p))
            hook.Run("PlayerHandleAnimEvent", ply, anim_event_id(p), p.time or 0, p.cycle or 0, p.type or 0, p.options or "")
        end
    end))
    -- the gravity gun's punt: GM:GravGunPunt then the punted scripted entity's own callback
    -- (the server has launched the entity already; a false return cannot stop it here)
    events.on("gravgun.punt", safe(function(p)
        local ply, ent = Player(p.player or 0), Entity(p.entity or 0)
        hook.Run("GravGunPunt", ply, ent)
        if IsValid(ent) and ent.GravGunPunt then ent:GravGunPunt(ply) end -- @hook ENTITY:GravGunPunt
    end))
    events.on("entity.fired_bullets", safe(function(p)
        local e = Entity(p.entity or p.attacker or 0)
        if IsValid(e) and e.DoImpactEffect and p.tr then e:DoImpactEffect(p.tr, p.damage_type or DMG_BULLET) end -- @hook ENTITY:DoImpactEffect
        -- a player's shot: the scripted weapon in hand gets the impact
        -- @hook WEAPON:DoImpactEffect
        local held = IsValid(e) and not e:IsWeapon() and e.GetActiveWeapon and e:GetActiveWeapon()
        if IsValid(held) and held.DoImpactEffect and p.tr then held:DoImpactEffect(p.tr, p.damage_type or DMG_BULLET) end
        if IsValid(e) and e.ShootEffects and e:IsWeapon() then e:ShootEffects() end -- @hook WEAPON:ShootEffects
        local owner = IsValid(e) and e.GetOwner and e:GetOwner()
        if IsValid(owner) and owner:IsPlayer() then hook.Run("DoAnimationEvent", owner, PLAYERANIMEVENT_ATTACK_PRIMARY) end
    end))
    events.on("npc.shoot_params", safe(function(p)
        local w = Entity(p.weapon or 0)
        if IsValid(w) then
            if w.GetNPCBurstSettings then w:GetNPCBurstSettings() end -- @hook WEAPON:GetNPCBurstSettings
            if w.GetNPCRestTimes then w:GetNPCRestTimes() end -- @hook WEAPON:GetNPCRestTimes
            if p.secondary and w.NPCShoot_Secondary then w:NPCShoot_Secondary(Entity(p.entity or 0), Entity(p.target or 0)) end -- @hook WEAPON:NPCShoot_Secondary
        end
    end))
    events.on("sandbox.spawn_request", safe(function(p)
        local ply = Player(p.player)
        if p.kind == "effect" or p.kind == "effects" then
            if hook.Run("PlayerSpawnEffect", ply, p.model or "") == false then return false end
            hook.Run("PlayerSpawnedEffect", ply, p.model or "", NULL)
        elseif p.model and p.model ~= "" then
            if hook.Run("PlayerSpawnObject", ply, p.model, p.skin or 0) == false then return false end
        end
    end))
    events.on("player.property_request", safe(function(p)
        local ply, e = Player(p.player), Entity(p.entity or 0)
        if p.name == "drive" and hook.Run("CanDrive", ply, e) == false then return false end
        if p.name == "arm_dupe" and hook.Run("CanArmDupe", ply, e) == false then return false end
    end))
    -- GM:SetupMove / Move / FinishMove hand the player class its turn (PLAYER:StartMove / Move / FinishMove)
    local base_setup, base_move, base_finish = GM.SetupMove, GM.Move, GM.FinishMove
    function GM:SetupMove(ply, mv, cmd) if base_setup then base_setup(self, ply, mv, cmd) end if drive.StartMove(ply, mv, cmd) then return end run_class(ply, "StartMove", mv, cmd) end -- @hook PLAYER:StartMove
    function GM:Move(ply, mv) if base_move and base_move(self, ply, mv) then return true end if drive.Move(ply, mv) then return true end return run_class(ply, "Move", mv) end -- @hook PLAYER:Move
    function GM:FinishMove(ply, mv) if base_finish then base_finish(self, ply, mv) end if drive.FinishMove(ply, mv) then return true end return run_class(ply, "FinishMove", mv) end -- @hook PLAYER:FinishMove
    function GM:PlayerSetHandsModel(ply, hands)
        local info = run_class(ply, "GetHandsModel") -- @hook PLAYER:GetHandsModel
        if not info and player_manager and player_manager.TranslatePlayerHands then info = player_manager.TranslatePlayerHands(player_manager.TranslateToPlayerModelName(ply:GetModel())) end
        if info and IsValid(hands) then hands:SetModel(info.model); hands:SetSkin(info.skin or 0); hands:SetBodyGroups(info.body or "0000000") end
    end
    function GM:PlayerSpawn(ply, transition)
        if ply:Team() == TEAM_SPECTATOR then hook.Run("PlayerSpawnAsSpectator", ply) return end
        if player_manager and player_manager.SetPlayerClass then player_manager.SetPlayerClass(ply, "player_default") end
        run_class(ply, "SetupDataTables") -- @hook PLAYER:SetupDataTables
        hook.Run("PlayerLoadout", ply)
        hook.Run("PlayerSetModel", ply)
        ply:SetupHands()
    end
end

------------------------------------------------------------------------ client bridges: frame hooks, menus, scoreboard, chat, death notices, pickups, target id
if CLIENT then
    local notices, pickups, scoreboard_open, spawnmenu_open, context_open = {}, {}, false, false, false
    local last_weapon
    function GM:AddDeathNotice(attacker, attackerTeam, inflictor, victim, victimTeam)
        table.insert(notices, { time = CurTime(), left = attacker, left_color = attackerTeam == -1 and table.Copy(Color(255, 80, 0)) or team.GetColor(attackerTeam), icon = inflictor, right = victim, right_color = victimTeam == -1 and Color(255, 80, 0) or team.GetColor(victimTeam) })
    end
    function GM:GetDeathNoticeEntityName(ent) if IsValid(ent) then return ent:IsPlayer() and ent:Nick() or ent:GetClass() end return "" end
    function GM:DrawDeathNotice(x, y)
        if GetConVar and GetConVar("cl_drawhud") and GetConVar("cl_drawhud"):GetInt() == 0 then return end
        local w, h = ScrW(), ScrH()
        local yy = y * h
        for i = #notices, 1, -1 do
            local n = notices[i]
            if CurTime() - n.time > 6 then table.remove(notices, i)
            else
                local text = n.left .. (n.icon ~= "" and ("  [" .. n.icon .. "]  ") or "  killed  ") .. n.right
                surface.SetFont("ChatFont"); local tw, th = surface.GetTextSize(text)
                local xx = x * w - tw
                draw.RoundedBox(6, xx - 6, yy - 4, tw + 12, th + 8, Color(0, 0, 0, 100))
                draw.SimpleText(n.left, "ChatFont", xx, yy, n.left_color)
                draw.SimpleText(string.sub(text, #n.left + 1), "ChatFont", xx + surface.GetTextSize(n.left), yy, n.right_color)
                yy = yy + th + 10
            end
        end
    end
    function GM:HUDWeaponPickedUp(wep) table.insert(pickups, { time = CurTime(), text = IsValid(wep) and (wep.GetPrintName and wep:GetPrintName() or wep:GetClass()) or tostring(wep), color = Color(255, 200, 0) }) end
    function GM:HUDItemPickedUp(name) table.insert(pickups, { time = CurTime(), text = tostring(name), color = Color(255, 200, 0) }) end
    function GM:HUDAmmoPickedUp(name, amount) table.insert(pickups, { time = CurTime(), text = tostring(amount) .. " " .. (language and language.GetPhrase and language.GetPhrase(tostring(name) .. "_ammo") or tostring(name)), color = Color(255, 200, 0) }) end
    function GM:HUDDrawPickupHistory()
        local y = ScrH() * 0.3
        for i = #pickups, 1, -1 do
            local p = pickups[i]
            if CurTime() - p.time > 5 then table.remove(pickups, i)
            else
                local a = math.Clamp((5 - (CurTime() - p.time)) * 2, 0, 1) * 255
                draw.SimpleText(p.text, "ChatFont", ScrW() - 20, y, Color(p.color.r, p.color.g, p.color.b, a), TEXT_ALIGN_RIGHT)
                y = y + 20
            end
        end
    end
    function GM:HUDDrawTargetID()
        local lp = LocalPlayer()
        local tr = lp:GetEyeTrace()
        if not tr or not IsValid(tr.Entity) or not tr.Entity:IsPlayer() then return end
        local text = tr.Entity:Nick()
        surface.SetFont("TargetID"); local w, h = surface.GetTextSize(text)
        local x, y = ScrW() / 2 - w / 2, ScrH() / 2 + 30
        draw.SimpleText(text, "TargetID", x + 1, y + 1, Color(0, 0, 0, 120))
        draw.SimpleText(text, "TargetID", x, y, hook.Run("GetTeamColor", tr.Entity))
        local hp = math.max(tr.Entity:Health(), 0) .. "%"
        local col = hook.Run("GetTeamNumColor", tr.Entity:Team())
        draw.SimpleText(hp, "TargetIDSmall", ScrW() / 2 - surface.GetTextSize(hp) / 2, y + h, col or Color(255, 255, 255))
    end
    function GM:HUDDrawScoreBoard() end
    GM.HUDPaint = safe(function()
        hook.Run("HUDDrawTargetID")
        hook.Run("HUDDrawPickupHistory")
        hook.Run("DrawDeathNotice", 0.85, 0.04)
        hook.Run("PaintNotes")
        hook.Run("PaintWorldTips")
    end)
    function GM:PaintNotes() if notification and notification.__paint then notification.__paint() end end
    function GM:PaintWorldTips() end
    function GM:ScoreboardShow() scoreboard_open = true; if GAMEMODE.ScoreboardPanel then GAMEMODE.ScoreboardPanel:Show() end return false end
    function GM:ScoreboardHide() scoreboard_open = false; if GAMEMODE.ScoreboardPanel then GAMEMODE.ScoreboardPanel:Hide() end end
    function GM:OnSpawnMenuOpen() spawnmenu_open = true end
    function GM:OnSpawnMenuClose() spawnmenu_open = false end
    function GM:OnContextMenuOpen() context_open = true end
    function GM:OnContextMenuClose() context_open = false end
    function GM:SpawnMenuEnabled() return true end
    function GM:SpawnMenuOpen() return true end
    function GM:SpawnMenuOpened() end
    function GM:ContextMenuEnabled() return true end
    function GM:ContextMenuOpen() return true end
    function GM:ContextMenuOpened() end
    function GM:ContextMenuClosed() end
    function GM:ContextMenuCreated(menu) end
    function GM:ContextMenuShowTool() return true end
    function GM:SpawnMenuCreated(menu) end
    function GM:ChatTextChanged(text) end
    function GM:OnChatTab(text) return text end
    function GM:OnPauseMenuShow() return true end
    function GM:OnPauseMenuBlockedTooManyTimes() end
    function GM:OnNotifyAddonConflict(a, b, c) end
    function GM:OnCloseCaptionEmit() end
    function GM:OnClientLuaError(err, realm, stack, name, id) end
    function GM:OnViewModelChanged(vm, old, new) end
    function GM:GUIMousePressed(code, aim) end
    function GM:GUIMouseReleased(code, aim) end
    function GM:GUIMouseDoublePressed(code, aim) end
    function GM:VGUIMousePressed(pnl, code) end
    function GM:VGUIMousePressAllowed(code) return true end
    function GM:PreventScreenClicks() return false end
    function GM:ForceDermaSkin() return nil end
    function GM:CaptureVideo() return nil end
    function GM:MenuStart() end
    function GM:StartGame() end
    function GM:CalcView(ply, origin, angles, fov, znear, zfar)
        local view = { origin = origin, angles = angles, fov = fov, znear = znear, zfar = zfar, drawviewer = false }
        if drive.CalcView(ply, view) then return view end
        if ply:InVehicle() then return hook.Run("CalcVehicleView", ply:GetVehicle(), ply, view) or view end
        local wep = ply:GetActiveWeapon()
        if IsValid(wep) and wep.CalcView then local o, a, f = wep:CalcView(ply, origin * 1, angles * 1, fov); view.origin, view.angles, view.fov = o or view.origin, a or view.angles, f or view.fov end -- @hook WEAPON:CalcView
        return view
    end
    function GM:CalcVehicleView(veh, ply, view) return view end
    function GM:CalcViewModelView(wep, vm, oldPos, oldAng, pos, ang)
        if IsValid(wep) and wep.GetViewModelPosition then local p, a = wep:GetViewModelPosition(pos * 1, ang * 1); pos, ang = p or pos, a or ang end -- @hook WEAPON:GetViewModelPosition
        if IsValid(wep) and wep.CalcViewModelView then local p, a = wep:CalcViewModelView(vm, oldPos, oldAng, pos, ang); pos, ang = p or pos, a or ang end -- @hook WEAPON:CalcViewModelView
        return pos, ang
    end
    function GM:CreateMove(cmd) if drive.CreateMove(cmd) then return end local wep = LocalPlayer():GetActiveWeapon(); if IsValid(wep) and wep.FreezeMovement and wep:FreezeMovement() then cmd:ClearMovement() end end
    function GM:AdjustMouseSensitivity(fov) local wep = LocalPlayer():GetActiveWeapon(); if IsValid(wep) and wep.AdjustMouseSensitivity then return wep:AdjustMouseSensitivity() end return nil end -- @hook WEAPON:AdjustMouseSensitivity
    function GM:InputMouseApply(cmd, x, y, ang) return false end
    function GM:GetMotionBlurValues(h, v, fwd, rot) return h, v, fwd, rot end
    function GM:NeedsDepthPass() return false end
    function GM:PostProcessPermitted(name) return true end
    function GM:SetupWorldFog() return false end
    function GM:SetupSkyboxFog(scale) return false end
    function GM:PreDrawSkyBox() return false end
    function GM:PostDrawSkyBox() end
    function GM:PostDraw2DSkyBox() end
    function GM:PreDrawViewModels() end
    function GM:PreDrawViewModel(vm, ply, wep) if IsValid(wep) and wep.PreDrawViewModel then return wep:PreDrawViewModel(vm, wep, ply) end return false end -- @hook WEAPON:PreDrawViewModel
    local function pdvm_wep(vm, ply, wep) if IsValid(wep) and wep.PostDrawViewModel then wep:PostDrawViewModel(vm, wep, ply) end if IsValid(wep) and wep.ViewModelDrawn then wep:ViewModelDrawn(vm) end end -- @hook WEAPON:PostDrawViewModel / WEAPON:ViewModelDrawn
    -- @hook WEAPON:ViewModelDrawn @hook PLAYER:PostDrawViewModel
    function GM:PostDrawViewModel(vm, ply, wep)
        pdvm_wep(vm, ply, wep)
        if IsValid(wep) and wep.ViewModelDrawn then wep:ViewModelDrawn(vm) end
        if player_manager and player_manager.RunClass then player_manager.RunClass(ply, "PostDrawViewModel", vm, wep) end
    end
    function GM:PreDrawPlayerHands(hands, vm, ply, wep) return false end
    function GM:PostDrawPlayerHands(hands, vm, ply, wep) end
    function GM:PrePlayerDraw(ply, flags) return false end
    function GM:PostPlayerDraw(ply, flags) end
    function GM:PreDrawHalos() end
    function GM:DrawPhysgunBeam(ply, physgun, enabled, target, physbone, hitpos) return true end
    function GM:DrawMonitors() end
    function GM:StartChat(teamSay) return false end
    function GM:FinishChat() end
    events.on("net.message", function() end)
    net.Receive("__gm_deathnotice", function()
        local a, at, inf, v, vt = net.ReadString(), net.ReadInt(16), net.ReadString(), net.ReadString(), net.ReadInt(16)
        hook.Run("AddDeathNotice", a, at, inf, v, vt)
    end)
    net.Receive("__gm_pickup", function()
        local kind, a, b = net.ReadString(), net.ReadString(), net.ReadInt(32)
        if kind == "weapon" then hook.Run("HUDWeaponPickedUp", a) elseif kind == "ammo" then hook.Run("HUDAmmoPickedUp", a, b) else hook.Run("HUDItemPickedUp", a) end
    end)
    -- the engine's per-frame view / command hooks, the weapon's HUD callbacks and tool screens
    events.on("tick.client", safe(function()
        local lp = LocalPlayer()
        if not IsValid(lp) then return end
        local cmd = lp:GetCurrentCommand()
        if cmd then
            hook.Run("CreateMove", cmd)
            hook.Run("InputMouseApply", cmd, cmd.GetMouseX and cmd:GetMouseX() or 0, cmd.GetMouseY and cmd:GetMouseY() or 0, lp:EyeAngles())
        end
        hook.Run("AdjustMouseSensitivity", lp:GetFOV())
        local wep = lp:GetActiveWeapon()
        local vm = lp:GetViewModel()
        if wep ~= last_weapon then hook.Run("OnViewModelChanged", vm, last_weapon and IsValid(last_weapon) and last_weapon:GetClass() or "", IsValid(wep) and wep:GetClass() or ""); run_class(lp, "ViewModelChanged", vm, "", IsValid(wep) and wep:GetClass() or "") last_weapon = wep end -- @hook PLAYER:ViewModelChanged
        local view = hook.Run("CalcView", lp, lp:EyePos(), lp:EyeAngles(), lp:GetFOV(), 3, 28377)
        if IsValid(wep) and wep.TranslateFOV then wep:TranslateFOV(view and view.fov or lp:GetFOV()) end -- @hook WEAPON:TranslateFOV
        hook.Run("CalcViewModelView", wep, vm, lp:EyePos(), lp:EyeAngles(), lp:EyePos(), lp:EyeAngles())
        hook.Run("GetMotionBlurValues", 0, 0, 0, 0)
        hook.Run("NeedsDepthPass")
        hook.Run("SetupWorldFog")
        hook.Run("SetupSkyboxFog", 1)
        animate(lp)
        if lp:InVehicle() then hook.Run("VehicleMove", lp, lp:GetVehicle(), nil) end
    end))
    events.on("render.hud", safe(function()
        local lp = LocalPlayer()
        if not IsValid(lp) then return end
        local wep = lp:GetActiveWeapon()
        local vm = lp:GetViewModel()
        if hook.Run("PreDrawSkyBox") ~= true then hook.Run("PostDraw2DSkyBox"); hook.Run("PostDrawSkyBox") end
        for _, ply in ipairs(player.GetAll()) do
            if ply ~= lp or hook.Run("ShouldDrawLocalPlayer", ply) then
                if hook.Run("PrePlayerDraw", ply, 0) ~= true then hook.Run("PostPlayerDraw", ply, 0) end
            end
        end
        for _, e in ipairs(ents.GetAll()) do
            if e.Draw and not e:IsPlayer() and (e.GetNoDraw == nil or not e:GetNoDraw()) then
                if e.RenderOverride then e:RenderOverride(0) else e:Draw(0) end -- @hook ENTITY:Draw / ENTITY:RenderOverride
                if e.DrawTranslucent then e:DrawTranslucent(0) end -- @hook ENTITY:DrawTranslucent
            end
            if e.DrawWorldModel and e:IsWeapon() and IsValid(e:GetOwner()) then e:DrawWorldModel(0) end -- @hook WEAPON:DrawWorldModel
            if e.DrawWorldModelTranslucent and e:IsWeapon() then e:DrawWorldModelTranslucent(0) end -- @hook WEAPON:DrawWorldModelTranslucent
        end
        hook.Run("PreDrawViewModels")
        if IsValid(wep) and (not wep.ShouldDrawViewModel or wep:ShouldDrawViewModel()) then -- @hook WEAPON:ShouldDrawViewModel
            if hook.Run("PreDrawViewModel", vm, lp, wep) ~= true then
                if run_class(lp, "PreDrawViewModel", vm, wep) ~= true then run_class(lp, "PostDrawViewModel", vm, wep) end -- @hook PLAYER:PreDrawViewModel / PLAYER:PostDrawViewModel
                hook.Run("PostDrawViewModel", vm, lp, wep)
            end
            local hands = lp:GetHands()
            if hook.Run("PreDrawPlayerHands", hands, vm, lp, wep) ~= true then hook.Run("PostDrawPlayerHands", hands, vm, lp, wep) end
        end
        hook.Run("PreDrawHalos")
        hook.Run("DrawMonitors")
        if IsValid(wep) and wep:GetClass() == "weapon_physgun" then
            local tr = lp:GetEyeTrace()
            hook.Run("DrawPhysgunBeam", lp, wep, lp:KeyDown(IN_ATTACK), tr and tr.Entity or NULL, 0, tr and tr.HitPos or lp:EyePos())
        end
        if IsValid(wep) then
            if wep.RenderScreen then wep:RenderScreen() end -- @hook WEAPON:RenderScreen
            if wep.HUDShouldDraw then for _, name in ipairs({ "CHudHealth", "CHudBattery", "CHudAmmo", "CHudSecondaryAmmo", "CHudCrosshair" }) do wep:HUDShouldDraw(name) end end -- @hook WEAPON:HUDShouldDraw
            if wep.DrawHUDBackground then wep:DrawHUDBackground() end -- @hook WEAPON:DrawHUDBackground
            if wep.DrawHUD then wep:DrawHUD() end -- @hook WEAPON:DrawHUD
            if wep.DoDrawCrosshair then wep:DoDrawCrosshair(ScrW() / 2, ScrH() / 2) end -- @hook WEAPON:DoDrawCrosshair
            if wep.CustomAmmoDisplay then wep:CustomAmmoDisplay() end -- @hook WEAPON:CustomAmmoDisplay
            if wep.PrintWeaponInfo and wep.__info_shown == nil then wep.__info_shown = true; wep:PrintWeaponInfo(ScrW() - 300, ScrH() * 0.3, 255) end -- @hook WEAPON:PrintWeaponInfo
            if wep.DrawWeaponSelection and lp.__weapon_selection_open then wep:DrawWeaponSelection(ScrW() - 300, ScrH() * 0.3, 250, 100, 255) end -- @hook WEAPON:DrawWeaponSelection
            local tool = lp.GetTool and lp:GetTool()
            if tool and tool.DrawHUD then tool:DrawHUD() end -- @hook TOOL:DrawHUD
            if tool and tool.DrawToolScreen then tool:DrawToolScreen(256, 256) end -- @hook TOOL:DrawToolScreen
        end
        if scoreboard_open then hook.Run("HUDDrawScoreBoard") end
    end))
    -- key hooks: scoreboard, spawn / context menus, pause menu, chat tab, GUI mouse
    events.on("input.key", safe(function(p)
        local bind = input.LookupKeyBinding(p.key) or ""
        local lp = LocalPlayer()
        if bind == "+showscores" then
            if p.down then hook.Run("ScoreboardShow") else hook.Run("ScoreboardHide") end
        elseif bind == "+menu" then
            if p.down then
                if hook.Run("SpawnMenuEnabled") ~= false and hook.Run("SpawnMenuOpen") ~= false then hook.Run("OnSpawnMenuOpen"); hook.Run("SpawnMenuOpened") end
            else hook.Run("OnSpawnMenuClose") end
        elseif bind == "+menu_context" then
            if p.down then
                if hook.Run("ContextMenuEnabled") ~= false and hook.Run("ContextMenuOpen") ~= false then hook.Run("OnContextMenuOpen"); hook.Run("ContextMenuOpened") end
            else hook.Run("OnContextMenuClose"); hook.Run("ContextMenuClosed") end
        elseif bind == "cancelselect" or p.key == "Escape" then
            if p.down then
                -- a gamemode blocking the pause menu three times running is overruled (GM:OnPauseMenuBlockedTooManyTimes)
                if hook.Run("OnPauseMenuShow") == false then
                    __pause_blocked = (__pause_blocked or 0) + 1
                    if __pause_blocked >= 3 then __pause_blocked = 0; hook.Run("OnPauseMenuBlockedTooManyTimes") end
                else
                    __pause_blocked = 0
                end
            end
        elseif bind == "messagemode" or bind == "messagemode2" then
            if p.down and chat and chat.Open then chat.Open(bind == "messagemode2" and 2 or 1) end
        elseif bind == "invprev" or bind == "invnext" or bind == "lastinv" or string.sub(bind, 1, 4) == "slot" then
            if IsValid(lp) then lp.__weapon_selection_open = p.down and CurTime() or lp.__weapon_selection_open end
        end
        if chat and chat.IsOpen and chat.IsOpen() and p.down then
            if p.key == "Tab" then hook.Run("OnChatTab", "") elseif p.key == "Escape" then chat.Close() end
        end
        if string.sub(p.key, 1, 5) == "Mouse" and (spawnmenu_open or context_open) then
            local code = MOUSE_LEFT or 107
            if hook.Run("PreventScreenClicks") ~= true and hook.Run("VGUIMousePressAllowed", code) ~= false then
                if p.down then hook.Run("GUIMousePressed", code, lp:GetAimVector()); hook.Run("VGUIMousePressed", vgui and vgui.GetHoveredPanel and vgui.GetHoveredPanel() or NULL, code)
                else hook.Run("GUIMouseReleased", code, lp:GetAimVector()) end
                if p.down and lp.__last_click and CurTime() - lp.__last_click < 0.3 then hook.Run("GUIMouseDoublePressed", code, lp:GetAimVector()) end
                if p.down then lp.__last_click = CurTime() end
            end
        end
    end))
    events.on("lua.error", function(p) hook.Run("OnClientLuaError", p.message or p.error or "", "client", p.stack or "", p.addon or "", 0) end)
    events.on("entity.created", safe(function(p)
        local e = Entity(p.entity or 0)
        hook.Run("NetworkEntityCreated", e)
        hook.Run("NotifyShouldTransmit", e, true)
        if IsValid(e) and e:GetClass() == "prop_ragdoll" and IsValid(e:GetOwner()) and e:GetOwner():IsPlayer() then hook.Run("CreateClientsideRagdoll", e:GetOwner(), e) end
    end))
    events.on("entity.removed", function(p) hook.Run("NotifyShouldTransmit", Entity(p.entity or 0), false) end)
    events.on("audio.state", function(p) if p.player then local ply = Player(p.player); if p.speaking then hook.Run("PlayerStartVoice", ply) else hook.Run("PlayerEndVoice", ply) end end end)
    -- the spawnmenu / tool menu population pass of the sandbox client (once the gamemode is up)
    local populated = false
    events.on("tick.client", safe(function()
        if populated then return end
        populated = true
        local skin = hook.Run("ForceDermaSkin")
        if skin and derma and derma.__force_skin then derma.__force_skin(skin) end
        if hook.Run("SpawnMenuEnabled") == false then return end
        local function panel(kind) local ok, p = pcall(vgui.Create, kind); return ok and p or nil end
        local menu, tree, node = panel("DPanel"), panel("DTree"), panel("DPanel")
        hook.Run("SpawnMenuCreated", menu)
        hook.Run("ContextMenuCreated", panel("DPanel"))
        hook.Run("PreReloadToolsMenu")
        hook.Run("AddGamemodeToolMenuTabs")
        hook.Run("AddToolMenuTabs")
        hook.Run("AddGamemodeToolMenuCategories")
        hook.Run("AddToolMenuCategories")
        hook.Run("PopulateToolMenu")
        hook.Run("PostReloadToolsMenu")
        hook.Run("PopulateContent", menu, tree, node)
        hook.Run("PopulateEntities", menu, tree, node)
        hook.Run("PopulateNPCs", menu, tree, node)
        hook.Run("PopulateVehicles", menu, tree, node)
        hook.Run("PopulateWeapons", menu, tree, node)
        hook.Run("ContentSidebarSelection", menu, node)
        if __glua_tools then for name, t in pairs(__glua_tools) do if t.BuildCPanel then t.BuildCPanel(panel("DForm")) end end end -- @hook TOOL:BuildCPanel
        hook.Run("MenuStart")
    end))
    function GM:PopulateToolMenu() end
    function GM:AddToolMenuTabs() end
    function GM:AddToolMenuCategories() end
    function GM:AddGamemodeToolMenuTabs() end
    function GM:AddGamemodeToolMenuCategories() end
    function GM:PreReloadToolsMenu() end
    function GM:PostReloadToolsMenu() end
    function GM:PopulateContent(pnl, tree, node) end
    function GM:PopulateEntities(pnl, tree, node) end
    function GM:PopulateNPCs(pnl, tree, node) end
    function GM:PopulateVehicles(pnl, tree, node) end
    function GM:PopulateWeapons(pnl, tree, node) end
    function GM:ContentSidebarSelection(pnl, node) end
    function GM:PreRegisterTOOL(tool, class) return true end
    function GM:SpawniconGenerated(path, panel, info) end
    function GM:SpawnlistOpenGenericMenu(canvas) end
    function GM:SpawnmenuIconMenuOpen(menu, icon, kind) end
    function GM:OnSaveSpawnlist() end
    function GM:OnRevertSpawnlist() end
    function GM:OpenToolbox() end
end
if MENU then
    events.on("tick.client", function() if not __gm_menu_started then __gm_menu_started = true; hook.Run("MenuStart") end end)
    events.on("game.start", function() hook.Run("StartGame") end)
    function GM:MenuStart() end
    function GM:StartGame() end
end
-- scripted entities: automatic frame advance is a plain setting the engine keeps
if __glua_classes and __glua_classes.Entity then
    function __glua_classes.Entity:SetAutomaticFrameAdvance(on) local v = rawget(self, "__vars"); if v then v.__auto_frame = on ~= false end end -- @hook ENTITY:SetAutomaticFrameAdvance
    function __glua_classes.Entity:SetCustomCollisionCheck(on) local v = rawget(self, "__vars"); if v then v.__custom_collide = on == true end end
    function __glua_classes.Entity:GetCustomCollisionCheck() local v = rawget(self, "__vars"); return v ~= nil and v.__custom_collide == true end
end

-- the native spawn menu's own menus reach the sandbox hooks; the panels are the engine's, not
-- Lua panels, so the panel arguments are nil
if CLIENT then
    events.on("spawnmenu.icon_menu", function(p) hook.Run("SpawnmenuIconMenuOpen", nil, nil, p.kind or "") end)
    events.on("spawnmenu.generic_menu", function() hook.Run("SpawnlistOpenGenericMenu", nil) end)
    events.on("spawnmenu.toolbox", function() hook.Run("OpenToolbox") end)
end
