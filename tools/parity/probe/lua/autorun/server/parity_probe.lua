-- Parity probe: executes scenarios from data/parity/scenarios and writes traces to data/parity/out.
-- Server realm. Kept deliberately small; every scenario kind is one function in SCENARIOS.

local OUT_DIR = "parity/out"
local SCN_DIR = "parity/scenarios"

local function log(fmt, ...)
	MsgN("[parity] " .. string.format(fmt, ...))
end

-- Recursive file.Find over a search path. Returns a sorted array of full paths.
local function findRecursive(dir, namePattern, searchPath, out)
	out = out or {}
	local files, dirs = file.Find(dir .. "/" .. namePattern, searchPath)
	for _, f in ipairs(files or {}) do
		out[#out + 1] = string.lower(dir .. "/" .. f)
	end
	local _, subdirs = file.Find(dir .. "/*", searchPath)
	for _, d in ipairs(subdirs or {}) do
		findRecursive(dir .. "/" .. d, namePattern, searchPath, out)
	end
	return out
end

local function findFlat(dir, namePattern, searchPath)
	local out = {}
	local files = file.Find(dir .. "/" .. namePattern, searchPath)
	for _, f in ipairs(files or {}) do
		out[#out + 1] = string.lower(dir .. "/" .. f)
	end
	return out
end

local SCENARIOS = {}
local ASYNC = { move = true, phys = true, physgun = true, constraint = true }

-- { kind = "file_find", globs = { "models/props_c17/*.mdl", "materials/**/*.vmt" }, search = "GAME" }
SCENARIOS.file_find = function(scn)
	local result = {}
	for _, glob in ipairs(scn.globs) do
		local dir, name = string.match(glob, "^(.-)/([^/]+)$")
		if not dir then dir, name = "", glob end
		local list
		local recDir = string.match(dir, "^(.-)/%*%*$")
		if recDir then
			list = findRecursive(recDir, name, scn.search or "GAME")
		elseif dir == "**" then
			list = findRecursive("", name, scn.search or "GAME")
		else
			list = findFlat(dir, name, scn.search or "GAME")
		end
		table.sort(list)
		result[glob] = list
	end
	return result
end

-- { kind = "convars", names = { "sv_gravity", ... } }  (empty names = a default list)
SCENARIOS.convars = function(scn)
	local out = {}
	for _, n in ipairs(scn.names) do
		local cv = GetConVar(n)
		out[n] = cv and cv:GetString() or false
	end
	return out
end

-- { kind = "info" }: engine facts that other harnesses depend on
SCENARIOS.info = function()
	return {
		tickinterval = engine.TickInterval(),
		tickcount = engine.TickCount(),
		map = game.GetMap(),
		version = VERSION,
		versionstr = VERSIONSTR,
		branch = BRANCH,
		jit = jit and jit.version or false,
		maxplayers = game.MaxPlayers(),
		hull = { player.GetAll()[1] and player.GetAll()[1]:OBBMaxs() or false },
	}
end


-- { kind = "move", start = {x,y,z}, angles = {pitch,yaw}, segments = { {ticks=100, buttons=8, forward=200, side=0, up=0, pitch=0, yaw=90}, ... } }
-- Drives a server bot through the real movement code with scripted usercmds and
-- records origin, velocity and ground state after every tick. Asynchronous.
SCENARIOS.move = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.move(scn, done) end)
		return
	end
	if not bot:Alive() then bot:Spawn() end
	bot:StripWeapons()
	local start = Vector(scn.start[1], scn.start[2], scn.start[3])
	local ang = Angle(scn.angles and scn.angles[1] or 0, scn.angles and scn.angles[2] or 0, 0)
	bot:SetPos(start)
	bot:SetEyeAngles(ang)
	bot:SetLocalVelocity(Vector(0, 0, 0))
	bot:SetMoveType(MOVETYPE_WALK)
	bot:SetDuckSpeed(0.4)
	local segs = scn.segments
	local total = 0
	for _, sg in ipairs(segs) do total = total + sg.ticks end
	local settle = 20
	local tick = -settle
	local ticks = {}
	local function segmentAt(t)
		local acc = 0
		for _, sg in ipairs(segs) do
			acc = acc + sg.ticks
			if t < acc then return sg end
		end
		return nil
	end
	hook.Add("StartCommand", "parity_move", function(ply, cmd)
		if ply ~= bot then return end
		local sg = tick >= 0 and segmentAt(tick) or nil
		local a = sg and Angle(sg.pitch or 0, sg.yaw or 0, 0) or ang
		cmd:ClearButtons()
		cmd:ClearMovement()
		cmd:SetButtons(sg and sg.buttons or 0)
		cmd:SetForwardMove(sg and sg.forward or 0)
		cmd:SetSideMove(sg and sg.side or 0)
		cmd:SetUpMove(sg and sg.up or 0)
		cmd:SetViewAngles(a)
	end)
	hook.Add("SetupMove", "parity_move", function(ply, mv, cmd)
		if ply ~= bot then return end
		if tick < 0 then
			mv:SetOrigin(start)
			mv:SetVelocity(Vector(0, 0, 0))
			mv:SetButtons(0)
			mv:SetForwardSpeed(0)
			mv:SetSideSpeed(0)
			mv:SetUpSpeed(0)
			mv:SetMoveAngles(ang)
			mv:SetAngles(ang)
			return
		end
		local sg = segmentAt(tick)
		if not sg then return end
		local a = Angle(sg.pitch or 0, sg.yaw or 0, 0)
		mv:SetButtons(sg.buttons or 0)
		mv:SetOldButtons(tick > 0 and (segmentAt(tick - 1) or {}).buttons or 0)
		mv:SetForwardSpeed(sg.forward or 0)
		mv:SetSideSpeed(sg.side or 0)
		mv:SetUpSpeed(sg.up or 0)
		mv:SetMoveAngles(a)
		mv:SetAngles(a)
		ply:SetEyeAngles(a)
	end)
	hook.Add("FinishMove", "parity_move", function(ply, mv)
		if ply ~= bot then return end
		if tick >= 0 then
			local p, v = mv:GetOrigin(), mv:GetVelocity()
			ticks[#ticks + 1] = {
				t = tick,
				pos = { p.x, p.y, p.z },
				vel = { v.x, v.y, v.z },
				ground = ply:IsOnGround(),
				ducked = ply:Crouching(),
				water = ply:WaterLevel(),
				speed = ply:GetMaxSpeed(),
			}
		end
		tick = tick + 1
		if tick >= total then
			hook.Remove("StartCommand", "parity_move")
			hook.Remove("SetupMove", "parity_move")
			hook.Remove("FinishMove", "parity_move")
			local p = ply:GetPos()
			done({
				start = { start.x, start.y, start.z },
				ticks = ticks,
				final = { p.x, p.y, p.z },
				hull = { mins = { ply:OBBMins().x, ply:OBBMins().y, ply:OBBMins().z }, maxs = { ply:OBBMaxs().x, ply:OBBMaxs().y, ply:OBBMaxs().z } },
				speeds = { walk = ply:GetWalkSpeed(), run = ply:GetRunSpeed(), crouched = ply:GetCrouchedWalkSpeed(), jump = ply:GetJumpPower(), duck = ply:GetDuckSpeed(), unduck = ply:GetUnDuckSpeed(), step = ply:GetStepSize() },
			})
		end
	end)
end

-- phys: spawn a prop_physics, optionally give it a velocity, record its transform every tick
SCENARIOS.phys = function(scn, done)
	local start = Vector(scn.start[1], scn.start[2], scn.start[3])
	local ang = Angle(scn.angles and scn.angles[1] or 0, scn.angles and scn.angles[2] or 0, scn.angles and scn.angles[3] or 0)
	local ent = ents.Create("prop_physics")
	ent:SetModel(scn.model)
	ent:SetPos(start)
	ent:SetAngles(ang)
	ent:Spawn()
	ent:Activate()
	local phys = ent:GetPhysicsObject()
	if not IsValid(phys) then
		done({ error = "no physics object for " .. tostring(scn.model) })
		ent:Remove()
		return
	end
	phys:Wake()
	if scn.velocity then phys:SetVelocity(Vector(scn.velocity[1], scn.velocity[2], scn.velocity[3])) end
	if scn.angular then phys:AddAngleVelocity(Vector(scn.angular[1], scn.angular[2], scn.angular[3])) end
	local info = {
		mass = phys:GetMass(),
		material = phys:GetMaterial(),
		volume = phys:GetVolume(),
		surface_area = phys:GetSurfaceArea(),
		inertia = { phys:GetInertia().x, phys:GetInertia().y, phys:GetInertia().z },
		mass_center = { phys:GetMassCenter().x, phys:GetMassCenter().y, phys:GetMassCenter().z },
		damping = { phys:GetDamping() },
		aabb = { mins = { phys:GetAABB() }, },
		mesh_convexes = #phys:GetMeshConvexes(),
	}
	local mins, maxs = phys:GetAABB()
	info.aabb = { mins = { mins.x, mins.y, mins.z }, maxs = { maxs.x, maxs.y, maxs.z } }
	local total = scn.ticks or 200
	local tick, ticks = 0, {}
	-- every impact the engine resolves: point, normal, speed and the velocities before it
	local collisions = {}
	ent:AddCallback("PhysicsCollide", function(_, d)
		collisions[#collisions + 1] = {
			t = tick,
			pos = { d.HitPos.x, d.HitPos.y, d.HitPos.z },
			normal = { d.HitNormal.x, d.HitNormal.y, d.HitNormal.z },
			speed = d.Speed,
			our_old_vel = { d.OurOldVelocity.x, d.OurOldVelocity.y, d.OurOldVelocity.z },
			their_old_vel = { d.TheirOldVelocity.x, d.TheirOldVelocity.y, d.TheirOldVelocity.z },
			delta_time = d.DeltaTime,
			hit = IsValid(d.HitEntity) and d.HitEntity:GetClass() or "world",
		}
	end)
	hook.Add("Tick", "parity_phys", function()
		if not IsValid(ent) then return end
		local p, a, v, av = phys:GetPos(), phys:GetAngles(), phys:GetVelocity(), phys:GetAngleVelocity()
		ticks[#ticks + 1] = {
			t = tick,
			pos = { p.x, p.y, p.z },
			ang = { a.p, a.y, a.r },
			vel = { v.x, v.y, v.z },
			angvel = { av.x, av.y, av.z },
			asleep = phys:IsAsleep(),
		}
		tick = tick + 1
		if tick >= total then
			hook.Remove("Tick", "parity_phys")
			ent:Remove()
			done({ start = { start.x, start.y, start.z }, model = scn.model, info = info, ticks = ticks, collisions = collisions })
		end
	end)
end

-- physgun: a bot holding weapon_physgun drives IN_ATTACK and view angles over a prop,
-- the prop's transform and the bot's eye are recorded every tick
SCENARIOS.physgun = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.physgun(scn, done) end)
		return
	end
	if not bot:Alive() then bot:Spawn() end
	bot:StripWeapons()
	bot:Give("weapon_physgun")
	bot:SelectWeapon("weapon_physgun")
	local start = Vector(scn.start[1], scn.start[2], scn.start[3])
	local ang = Angle(scn.angles and scn.angles[1] or 0, scn.angles and scn.angles[2] or 0, 0)
	bot:SetPos(start)
	bot:SetEyeAngles(ang)
	bot:SetLocalVelocity(Vector(0, 0, 0))
	bot:SetMoveType(MOVETYPE_WALK)
	local ent = ents.Create("prop_physics")
	ent:SetModel(scn.model)
	ent:SetPos(Vector(scn.prop[1], scn.prop[2], scn.prop[3]))
	ent:SetAngles(Angle(scn.prop_angles and scn.prop_angles[1] or 0, scn.prop_angles and scn.prop_angles[2] or 0, scn.prop_angles and scn.prop_angles[3] or 0))
	ent:Spawn()
	ent:Activate()
	local phys = ent:GetPhysicsObject()
	local segs = scn.segments
	local total = 0
	for _, sg in ipairs(segs) do total = total + sg.ticks end
	local settle = 60
	local tick = -settle
	local ticks = {}
	local function segmentAt(t)
		local acc = 0
		for _, sg in ipairs(segs) do
			acc = acc + sg.ticks
			if t < acc then return sg end
		end
		return nil
	end
	hook.Add("StartCommand", "parity_physgun", function(ply, cmd)
		if ply ~= bot then return end
		local sg = tick >= 0 and segmentAt(tick) or nil
		local a = sg and Angle(sg.pitch or 0, sg.yaw or 0, 0) or ang
		cmd:ClearButtons()
		cmd:ClearMovement()
		cmd:SetButtons(sg and sg.buttons or 0)
		cmd:SetViewAngles(a)
		cmd:SetForwardMove(sg and sg.forward or 0)
		cmd:SetSideMove(sg and sg.side or 0)
		cmd:SetMouseX(sg and sg.mouse_x or 0)
		cmd:SetMouseY(sg and sg.mouse_y or 0)
	end)
	hook.Add("SetupMove", "parity_physgun", function(ply, mv, cmd)
		if ply ~= bot then return end
		if tick < 0 then
			mv:SetOrigin(start)
			mv:SetVelocity(Vector(0, 0, 0))
			if tick == -30 then bot:SelectWeapon("weapon_physgun") end
		end
	end)
	hook.Add("FinishMove", "parity_physgun", function(ply, mv)
		if ply ~= bot then return end
		if tick >= 0 and IsValid(ent) and IsValid(phys) then
			local p, a, v, av = phys:GetPos(), phys:GetAngles(), phys:GetVelocity(), phys:GetAngleVelocity()
			local e, ea = ply:EyePos(), ply:EyeAngles()
			ticks[#ticks + 1] = {
				t = tick,
				pos = { p.x, p.y, p.z },
				ang = { a.p, a.y, a.r },
				vel = { v.x, v.y, v.z },
				angvel = { av.x, av.y, av.z },
				held = ent:IsPlayerHolding(),
				frozen = not phys:IsMotionEnabled(),
				weapon = IsValid(ply:GetActiveWeapon()) and ply:GetActiveWeapon():GetClass() or "",
				eye = { e.x, e.y, e.z },
				eye_ang = { ea.p, ea.y, ea.r },
			}
		end
		tick = tick + 1
		if tick >= total then
			hook.Remove("StartCommand", "parity_physgun")
			hook.Remove("SetupMove", "parity_physgun")
			hook.Remove("FinishMove", "parity_physgun")
			local info = IsValid(phys) and { mass = phys:GetMass(), material = phys:GetMaterial() } or {}
			if IsValid(ent) then ent:Remove() end
			done({
				start = { start.x, start.y, start.z },
				model = scn.model,
				info = info,
				convars = {
					maxspeed = GetConVar("physgun_maxSpeed") and GetConVar("physgun_maxSpeed"):GetFloat(),
					maxangular = GetConVar("physgun_maxAngular") and GetConVar("physgun_maxAngular"):GetFloat(),
					damping = GetConVar("physgun_dampingFactor") and GetConVar("physgun_dampingFactor"):GetFloat(),
					timetoarrive = GetConVar("physgun_timeToArrive") and GetConVar("physgun_timeToArrive"):GetFloat(),
					teleport = GetConVar("physgun_teleportDistance") and GetConVar("physgun_teleportDistance"):GetFloat(),
				},
				ticks = ticks,
			})
		end
	end)
end

-- constraint: several props, constraint.* calls between them, every prop recorded per tick
SCENARIOS.constraint = function(scn, done)
	local ents_ = {}
	for i, pr in ipairs(scn.props) do
		local ent = ents.Create("prop_physics")
		ent:SetModel(pr.model)
		ent:SetPos(Vector(pr.pos[1], pr.pos[2], pr.pos[3]))
		ent:SetAngles(Angle(pr.angles and pr.angles[1] or 0, pr.angles and pr.angles[2] or 0, pr.angles and pr.angles[3] or 0))
		ent:Spawn()
		ent:Activate()
		local ph = ent:GetPhysicsObject()
		if IsValid(ph) then
			ph:Wake()
			if pr.frozen then ph:EnableMotion(false) end
		end
		ents_[i] = ent
	end
	local made = {}
	for _, c in ipairs(scn.constraints or {}) do
		local a, b = ents_[c.a], ents_[c.b]
		local la = c.local_a and Vector(c.local_a[1], c.local_a[2], c.local_a[3]) or Vector(0, 0, 0)
		local lb = c.local_b and Vector(c.local_b[1], c.local_b[2], c.local_b[3]) or Vector(0, 0, 0)
		local ok, err
		if c.kind == "weld" then
			ok = constraint.Weld(a, b, 0, 0, c.force or 0, c.nocollide or false, false)
		elseif c.kind == "rope" then
			ok = constraint.Rope(a, b, 0, 0, la, lb, c.length or 0, c.addlength or 0, c.force or 0, c.width or 1, "cable/rope", c.rigid or false)
		elseif c.kind == "axis" then
			ok = constraint.Axis(a, b, 0, 0, la, lb, c.force or 0, c.torque or 0, c.friction or 0, c.nocollide and 1 or 0, c.axis and Vector(c.axis[1], c.axis[2], c.axis[3]) or nil)
		elseif c.kind == "nocollide" then
			ok = constraint.NoCollide(a, b, 0, 0)
		end
		made[#made + 1] = { kind = c.kind, ok = IsValid(ok) and ok:GetClass() or tostring(ok) }
	end
	local total = scn.ticks or 200
	local tick, ticks = 0, {}
	hook.Add("Tick", "parity_constraint", function()
		local row = { t = tick, props = {} }
		for i, ent in ipairs(ents_) do
			local ph = IsValid(ent) and ent:GetPhysicsObject()
			if ph and IsValid(ph) then
				local p, a, v, av = ph:GetPos(), ph:GetAngles(), ph:GetVelocity(), ph:GetAngleVelocity()
				row.props[i] = { pos = { p.x, p.y, p.z }, ang = { a.p, a.y, a.r }, vel = { v.x, v.y, v.z }, angvel = { av.x, av.y, av.z }, asleep = ph:IsAsleep() }
			end
		end
		ticks[#ticks + 1] = row
		tick = tick + 1
		if tick >= total then
			hook.Remove("Tick", "parity_constraint")
			local info = {}
			for i, ent in ipairs(ents_) do
				local ph = IsValid(ent) and ent:GetPhysicsObject()
				info[i] = ph and IsValid(ph) and { mass = ph:GetMass(), material = ph:GetMaterial() } or {}
				if IsValid(ent) then ent:Remove() end
			end
			done({ constraints = made, info = info, ticks = ticks })
		end
	end)
end


-- sandbox: drive the real sandbox gamemode as a bot (gm_spawn, gmod_undo, gmod_cleanup, sbox_* limits)
-- and record what the server does after every op: props of the bot (pos, angles, frozen), the
-- spawn trace, the undo stack, the counts and every LimitHit / SendLua sent to the bot.
-- { kind = "sandbox", start = {x,y,z}, ops = { {op="spawn", model=..., pitch=30, yaw=0}, {op="undo"}, {op="cleanup"}, {op="cvar", name="sbox_maxprops", value="3"} } }
SCENARIOS.sandbox = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.sandbox(scn, done) end)
		return
	end
	if not bot:Alive() then bot:Spawn() end
	bot:SetPos(Vector(scn.start[1], scn.start[2], scn.start[3]))
	bot:SetLocalVelocity(Vector(0, 0, 0))
	for _, e in ipairs(ents.FindByClass("prop_physics")) do e:Remove() end
	undo.GetTable()[bot:UniqueID()] = nil
	-- props that are already on the table (not the bot's): spawn targets
	local placed = {}
	for i, pr in ipairs(scn.props or {}) do
		local ent = ents.Create("prop_physics")
		ent:SetModel(pr.model)
		ent:SetPos(Vector(pr.pos[1], pr.pos[2], pr.pos[3]))
		ent:SetAngles(Angle(pr.angles and pr.angles[1] or 0, pr.angles and pr.angles[2] or 0, pr.angles and pr.angles[3] or 0))
		ent:Spawn()
		ent:Activate()
		local ph = ent:GetPhysicsObject()
		if IsValid(ph) and pr.frozen then ph:EnableMotion(false) end
		placed[i] = ent
	end
	local meta = FindMetaTable("Player")
	local messages = {}
	local oldSendLua, oldLimitHit = meta.SendLua, meta.LimitHit
	meta.SendLua = function(ply, str) if ply == bot then messages[#messages + 1] = { t = engine.TickCount(), lua = str } end return oldSendLua(ply, str) end
	meta.LimitHit = function(ply, str) if ply == bot then messages[#messages + 1] = { t = engine.TickCount(), limit = str } end return oldLimitHit(ply, str) end
	local function snapshot()
		local props = {}
		for _, e in ipairs(ents.FindByClass("prop_physics")) do
			if e:GetCreator() == bot then
				local ph = e:GetPhysicsObject()
				local p, a = e:GetPos(), e:GetAngles()
				props[#props + 1] = { id = e:EntIndex(), model = e:GetModel(), pos = { p.x, p.y, p.z }, ang = { a.p, a.y, a.r },
					frozen = IsValid(ph) and not ph:IsMotionEnabled() or false, asleep = IsValid(ph) and ph:IsAsleep() or false,
					obb_mins = { e:OBBMins().x, e:OBBMins().y, e:OBBMins().z }, obb_maxs = { e:OBBMaxs().x, e:OBBMaxs().y, e:OBBMaxs().z } }
			end
		end
		table.sort(props, function(a, b) return a.id < b.id end)
		local undos = undo.GetTable()[bot:UniqueID()] or {}
		local ulist = {}
		for k, u in pairs(undos) do ulist[#ulist + 1] = { id = k, name = u.Name, text = u.NiceText, n = #u.Entities } end
		table.sort(ulist, function(a, b) return a.id < b.id end)
		return { props = props, count = bot:GetCount("props"), undos = ulist, maxprops = GetConVar("sbox_maxprops"):GetInt() }
	end
	local ops, results, i = scn.ops, {}, 0
	local function step()
		i = i + 1
		local op = ops[i]
		if not op then
			meta.SendLua, meta.LimitHit = oldSendLua, oldLimitHit
			local pl = {}
			for i, pe in ipairs(placed) do if IsValid(pe) then local p, a = pe:GetPos(), pe:GetAngles() pl[i] = { pos = { p.x, p.y, p.z }, ang = { a.p, a.y, a.r } } end end
			done({ results = results, messages = messages, placed = pl, eye = { bot:GetShootPos().x, bot:GetShootPos().y, bot:GetShootPos().z } })
			return
		end
		local rec = { op = op.op, t = engine.TickCount() }
		-- the table before the op: poses of the placed props and of everything spawned so far
		rec.before = { placed = {}, props = snapshot().props }
		for i, pe in ipairs(placed) do if IsValid(pe) then local p, a = pe:GetPos(), pe:GetAngles() rec.before.placed[i] = { pos = { p.x, p.y, p.z }, ang = { a.p, a.y, a.r } } end end
		if op.op == "spawn" then
			bot:SetEyeAngles(Angle(op.pitch or 0, op.yaw or 0, 0))
			local eye = bot:GetShootPos()
			local tr = util.TraceLine({ start = eye, endpos = eye + bot:EyeAngles():Forward() * 2048, filter = { bot } })
			local placedIndex = 0
			for i, pe in ipairs(placed) do if pe == tr.Entity then placedIndex = i end end
			rec.trace = { hit = tr.Hit, pos = { tr.HitPos.x, tr.HitPos.y, tr.HitPos.z }, normal = { tr.HitNormal.x, tr.HitNormal.y, tr.HitNormal.z }, ent = IsValid(tr.Entity) and tr.Entity:GetClass() or "world", placed = placedIndex, eye = { eye.x, eye.y, eye.z } }
			rec.model = op.model
			concommand.Run(bot, "gm_spawn", { op.model }, op.model)
		elseif op.op == "undo" then
			concommand.Run(bot, "gmod_undo", {}, "")
		elseif op.op == "cleanup" then
			concommand.Run(bot, "gmod_cleanup", {}, "")
		elseif op.op == "cvar" then
			RunConsoleCommand(op.name, op.value)
		end
		rec.immediate = snapshot()
		timer.Simple(engine.TickInterval() * (op.wait or 3), function()
			rec.after = snapshot()
			rec.t_after = engine.TickCount()
			results[#results + 1] = rec
			step()
		end)
	end
	timer.Simple(engine.TickInterval() * (scn.settle or 40), step)
end
ASYNC.sandbox = true


-- toolgun: props at rest, a bot with gmod_tool clicks real stools (weld, rope, ...) through the
-- weapon's own trace (DoToolTrace: line with the tool mask, hull fallback) and the tool object;
-- after every op the constraints (their ctable), props, undo stack, counts and messages are recorded.
-- { kind = "toolgun", start = {x,y,z}, props = { {model=..., pos={...}, angles={...}, frozen=false} },
--   ops = { {op="click", tool="weld", pitch=30, yaw=0, button="left"}, {op="undo"}, {op="wait", ticks=10}, {op="cvar", name=..., value=...} } }
SCENARIOS.toolgun = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.toolgun(scn, done) end)
		return
	end
	if not bot:Alive() then bot:Spawn() end
	bot:SetPos(Vector(scn.start[1], scn.start[2], scn.start[3]))
	bot:SetLocalVelocity(Vector(0, 0, 0))
	bot:StripWeapons()
	bot:Give("gmod_tool")
	bot:SelectWeapon("gmod_tool")
	-- a clean table: leftovers of earlier scenarios would collide with the props placed here
	for _, cls in ipairs({ "prop_physics", "phys_constraint", "phys_lengthconstraint", "keyframe_rope" }) do
		for _, e in ipairs(ents.FindByClass(cls)) do e:Remove() end
	end
	undo.GetTable()[bot:UniqueID()] = nil
	local props = {}
	for i, pr in ipairs(scn.props or {}) do
		local ent = ents.Create("prop_physics")
		ent:SetModel(pr.model)
		ent:SetPos(Vector(pr.pos[1], pr.pos[2], pr.pos[3]))
		ent:SetAngles(Angle(pr.angles and pr.angles[1] or 0, pr.angles and pr.angles[2] or 0, pr.angles and pr.angles[3] or 0))
		ent:Spawn()
		ent:Activate()
		local ph = ent:GetPhysicsObject()
		if IsValid(ph) then
			ph:Wake()
			if pr.frozen then ph:EnableMotion(false) end
		end
		props[i] = ent
	end
	local meta = FindMetaTable("Player")
	local messages = {}
	local oldSendLua, oldLimitHit = meta.SendLua, meta.LimitHit
	meta.SendLua = function(ply, str) if ply == bot then messages[#messages + 1] = { t = engine.TickCount(), lua = str } end return oldSendLua(ply, str) end
	meta.LimitHit = function(ply, str) if ply == bot then messages[#messages + 1] = { t = engine.TickCount(), limit = str } end return oldLimitHit(ply, str) end
	local function vec(v) if not v then return nil end return { v.x, v.y, v.z } end
	local function entref(e)
		if e == nil then return nil end
		if not IsValid(e) then if e == game.GetWorld() then return { index = 0, class = "worldspawn" } end return { index = -1 } end
		local idx = 0
		for i, pe in ipairs(props) do if pe == e then idx = i end end
		return { index = e:EntIndex(), class = e:GetClass(), prop = idx }
	end
	local function snapshot()
		local out = { props = {}, constraints = {}, ropes = {} }
		for i, e in ipairs(props) do
			if IsValid(e) then
				local ph = e:GetPhysicsObject()
				local p, a, v = e:GetPos(), e:GetAngles(), IsValid(ph) and ph:GetVelocity() or Vector(0, 0, 0)
				out.props[i] = { index = e:EntIndex(), pos = { p.x, p.y, p.z }, ang = { a.p, a.y, a.r }, vel = { v.x, v.y, v.z },
					frozen = IsValid(ph) and not ph:IsMotionEnabled() or false, asleep = IsValid(ph) and ph:IsAsleep() or false }
			else
				out.props[i] = { index = -1 }
			end
		end
		for _, cls in ipairs({ "phys_constraint", "phys_lengthconstraint", "phys_ballsocket", "phys_hinge", "phys_slideconstraint", "logic_collision_pair" }) do
			for _, c in ipairs(ents.FindByClass(cls)) do
				local t = c:GetTable()
				out.constraints[#out.constraints + 1] = { index = c:EntIndex(), class = cls, type = t.Type,
					ent1 = entref(t.Ent1), ent2 = entref(t.Ent2), bone1 = t.Bone1, bone2 = t.Bone2,
					lpos1 = vec(t.LPos1), lpos2 = vec(t.LPos2), length = t.length, addlength = t.addlength,
					forcelimit = t.forcelimit, nocollide = t.nocollide, rigid = t.rigid, width = t.width, material = t.material, torquelimit = t.torquelimit, friction = t.friction,
					pos = vec(c:GetPos()) }
			end
		end
		table.sort(out.constraints, function(a, b) return a.index < b.index end)
		for _, r in ipairs(ents.FindByClass("keyframe_rope")) do
			out.ropes[#out.ropes + 1] = { index = r:EntIndex(), pos = vec(r:GetPos()), length = r:GetKeyValues().Length, width = r:GetKeyValues().Width }
		end
		local undos = undo.GetTable()[bot:UniqueID()] or {}
		local ulist = {}
		for k, u in pairs(undos) do ulist[#ulist + 1] = { id = k, name = u.Name, text = u.NiceText, custom = u.CustomUndoText, n = #u.Entities } end
		table.sort(ulist, function(a, b) return a.id < b.id end)
		out.undos = ulist
		out.counts = { constraints = bot:GetCount("constraints"), ropeconstraints = bot:GetCount("ropeconstraints"), props = bot:GetCount("props") }
		local wep = bot:GetWeapon("gmod_tool")
		out.stage = IsValid(wep) and wep:GetNWInt("Stage", 0) or -1
		return out
	end
	local toolMask = bit.bor(CONTENTS_SOLID, CONTENTS_MOVEABLE, CONTENTS_MONSTER, CONTENTS_WINDOW, CONTENTS_DEBRIS, CONTENTS_GRATE, CONTENTS_AUX)
	local function toolTrace()
		local tr = util.GetPlayerTrace(bot)
		tr.mask = toolMask
		tr.mins = vector_origin
		tr.maxs = tr.mins
		tr.filter = { bot, bot:GetVehicle() }
		local trace = util.TraceLine(tr)
		if not trace.Hit or not IsValid(trace.Entity) then
			local hulltrace = util.TraceHull(tr)
			if IsValid(hulltrace.Entity) then
				trace = hulltrace
				trace.HullTrace = true
			end
		end
		return trace
	end
	local ops, results, i = scn.ops, {}, 0
	local function step()
		i = i + 1
		local op = ops[i]
		if not op then
			meta.SendLua, meta.LimitHit = oldSendLua, oldLimitHit
			local eye = bot:GetShootPos()
			done({ results = results, messages = messages, eye = { eye.x, eye.y, eye.z } })
			return
		end
		local rec = { op = op.op, t = engine.TickCount() }
		rec.before = snapshot().props
		local wait = op.wait or 3
		if op.op == "click" then
			bot:SetEyeAngles(Angle(op.pitch or 0, op.yaw or 0, 0))
			local tr = toolTrace()
			local eye = bot:GetShootPos()
			rec.tool, rec.button = op.tool, op.button or "left"
			rec.trace = { hit = tr.Hit, pos = vec(tr.HitPos), normal = vec(tr.HitNormal), start = vec(tr.StartPos), eye = { eye.x, eye.y, eye.z },
				ent = entref(tr.Entity), bone = tr.PhysicsBone, hull = tr.HullTrace or false, fraction = tr.Fraction }
			local wep = bot:GetWeapon("gmod_tool")
			local tool = IsValid(wep) and wep:GetToolObject(op.tool) or nil
			if tool and tr.Hit then
				local fn = (op.button == "right") and tool.RightClick or (op.button == "reload") and tool.Reload or tool.LeftClick
				local ok, ret = pcall(fn, tool, tr)
				rec.ok, rec.ret = ok, ok and ret or tostring(ret)
			else
				rec.ok, rec.ret = false, tool and "miss" or "no tool"
			end
		elseif op.op == "undo" then
			concommand.Run(bot, "gmod_undo", {}, "")
		elseif op.op == "cleanup" then
			concommand.Run(bot, "gmod_cleanup", {}, "")
		elseif op.op == "cvar" then
			RunConsoleCommand(op.name, op.value)
		elseif op.op == "wait" then
			wait = op.ticks or 10
		end
		rec.immediate = snapshot()
		timer.Simple(engine.TickInterval() * wait, function()
			rec.after = snapshot()
			rec.t_after = engine.TickCount()
			results[#results + 1] = rec
			step()
		end)
	end
	timer.Simple(engine.TickInterval() * (scn.settle or 40), step)
end
ASYNC.toolgun = true


-- player: a bot's life cycle. Spawned fresh, killed at the given ticks, recorded every server
-- tick (position, health, armour, weapons, ragdoll) with the gamemode's spawn/death events and
-- the map's spawn points, so the respawn rule and spawn point choice can be measured.
SCENARIOS.player = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.player(scn, done) end)
		return
	end
	local spawnClasses = { "info_player_start", "info_player_deathmatch", "info_player_combine", "info_player_rebel",
		"info_player_counterterrorist", "info_player_terrorist", "gmod_player_start", "info_player_allies", "info_player_axis" }
	local spawns = {}
	for _, cls in ipairs(spawnClasses) do
		for _, e in ipairs(ents.FindByClass(cls)) do
			local p, a = e:GetPos(), e:GetAngles()
			spawns[#spawns + 1] = { class = cls, pos = { p.x, p.y, p.z }, ang = { a.p, a.y, a.r }, name = e:GetName() }
		end
	end
	local killAt = {}
	for _, t in ipairs(scn.kill_at or {}) do killAt[t] = true end
	local pressAt = {}
	for _, t in ipairs(scn.press_at or {}) do pressAt[t] = true end
	local total = scn.ticks or 600
	local tick = -5
	local ticks, events = {}, {}
	local lastWeapons = ""
	local function weaponList(ply)
		local names = {}
		for _, w in ipairs(ply:GetWeapons()) do names[#names + 1] = w:GetClass() end
		table.sort(names)
		return table.concat(names, ",")
	end
	local function ev(name, ply, extra)
		if ply ~= bot then return end
		local p = ply:GetPos()
		local e = { t = tick, event = name, pos = { p.x, p.y, p.z }, alive = ply:Alive(), health = ply:Health() }
		for k, v in pairs(extra or {}) do e[k] = v end
		events[#events + 1] = e
	end
	hook.Add("PlayerSpawn", "parity_player", function(ply) ev("PlayerSpawn", ply) end)
	hook.Add("PlayerDeath", "parity_player", function(ply, inflictor, attacker)
		ev("PlayerDeath", ply, { attacker = IsValid(attacker) and attacker:GetClass() or tostring(attacker) })
	end)
	hook.Add("PostPlayerDeath", "parity_player", function(ply) ev("PostPlayerDeath", ply) end)
	hook.Add("PlayerSelectSpawn", "parity_player", function(ply) ev("PlayerSelectSpawn", ply) end)
	hook.Add("PlayerLoadout", "parity_player", function(ply) ev("PlayerLoadout", ply) end)
	hook.Add("PlayerSetModel", "parity_player", function(ply) ev("PlayerSetModel", ply) end)
	hook.Add("PlayerDeathThink", "parity_player", function(ply)
		if ply == bot and (ticks[#ticks] or {}).t ~= tick then
			-- note the first death-think tick of each death
			if not events.deathThinkMarked then
				events.deathThinkMarked = true
				ev("PlayerDeathThink", ply)
			end
		end
	end)
	hook.Add("StartCommand", "parity_player", function(ply, cmd)
		if ply ~= bot then return end
		cmd:ClearButtons()
		cmd:ClearMovement()
		if pressAt[tick] then cmd:SetButtons(IN_ATTACK) end
	end)
	bot:Spawn()
	hook.Add("Tick", "parity_player", function()
		if not IsValid(bot) then return end
		if tick >= 0 then
			if killAt[tick] then
				events.deathThinkMarked = false
				bot:Kill()
			end
			local p, v = bot:GetPos(), bot:GetVelocity()
			local w = weaponList(bot)
			local rag = bot:GetRagdollEntity()
			local aw = bot:GetActiveWeapon()
			local row = {
				t = tick,
				pos = { p.x, p.y, p.z },
				vel = { v.x, v.y, v.z },
				alive = bot:Alive(),
				health = bot:Health(),
				armor = bot:Armor(),
				movetype = bot:GetMoveType(),
				ragdoll = IsValid(rag),
				deaths = bot:Deaths(),
				frags = bot:Frags(),
				active = IsValid(aw) and aw:GetClass() or "",
				team = bot:Team(),
				obs = bot:GetObserverMode(),
			}
			if w ~= lastWeapons then
				row.weapons = w
				lastWeapons = w
			end
			if tick == 0 then
				row.model = bot:GetModel()
				row.maxhealth = bot:GetMaxHealth()
				row.hull = { mins = { bot:OBBMins().x, bot:OBBMins().y, bot:OBBMins().z }, maxs = { bot:OBBMaxs().x, bot:OBBMaxs().y, bot:OBBMaxs().z } }
			end
			if IsValid(rag) then
				local rp = rag:GetPos()
				row.ragdoll_pos = { rp.x, rp.y, rp.z }
			end
			ticks[#ticks + 1] = row
		end
		tick = tick + 1
		if tick >= total then
			for _, h in ipairs({ "PlayerSpawn", "PlayerDeath", "PostPlayerDeath", "PlayerSelectSpawn", "PlayerLoadout", "PlayerSetModel", "PlayerDeathThink", "StartCommand", "Tick" }) do
				hook.Remove(h, "parity_player")
			end
			events.deathThinkMarked = nil
			done({
				spawns = spawns,
				ticks = ticks,
				events = events,
				convars = {
					sbox_playershurtplayers = GetConVar("sbox_playershurtplayers") and GetConVar("sbox_playershurtplayers"):GetInt(),
					sbox_godmode = GetConVar("sbox_godmode") and GetConVar("sbox_godmode"):GetInt(),
					sbox_weapons = GetConVar("sbox_weapons") and GetConVar("sbox_weapons"):GetInt(),
					mp_falldamage = GetConVar("mp_falldamage") and GetConVar("mp_falldamage"):GetInt(),
					sv_gravity = GetConVar("sv_gravity") and GetConVar("sv_gravity"):GetInt(),
				},
			})
		end
	end)
end
ASYNC.player = true


-- weapon: a bot with the sandbox loadout; every weapon's clips and ammo on spawn, then
-- SelectWeapon at the scenario's ticks, the active weapon and its clips recorded every tick
-- (switch latency, deploy), the player's ammo per type.
SCENARIOS.weapon = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.weapon(scn, done) end)
		return
	end
	local start = Vector(scn.start[1], scn.start[2], scn.start[3])
	local selectAt = {}
	for _, e in ipairs(scn.select or {}) do selectAt[e[1]] = e[2] end
	local total = scn.ticks or 200
	local tick = -5
	local ticks, loadout = {}, {}
	local function ammoName(id) return id and id >= 0 and game.GetAmmoName(id) or "" end
	local function ammoTable(ply)
		local t = {}
		for id = 1, 32 do
			local n = game.GetAmmoName(id)
			if n then t[n] = ply:GetAmmoCount(id) end
		end
		return t
	end
	hook.Add("Tick", "parity_weapon", function()
		if tick == -5 then
			bot:Spawn()
			bot:SetPos(start)
			bot:SetEyeAngles(Angle(0, 90, 0))
		end
		if tick == 0 then
			for _, w in ipairs(bot:GetWeapons()) do
				loadout[#loadout + 1] = {
					class = w:GetClass(), clip1 = w:Clip1(), clip2 = w:Clip2(),
					maxclip1 = w:GetMaxClip1(), maxclip2 = w:GetMaxClip2(),
					ammo1 = ammoName(w:GetPrimaryAmmoType()), ammo2 = ammoName(w:GetSecondaryAmmoType()),
					ammo1_id = w:GetPrimaryAmmoType(), ammo2_id = w:GetSecondaryAmmoType(),
					slot = w:GetSlot(), slotpos = w:GetSlotPos(), weight = w:GetWeight(),
					holdtype = w:GetHoldType(), printname = w:GetPrintName(),
				}
			end
			table.sort(loadout, function(a, b) return a.class < b.class end)
		end
		if tick >= 0 then
			if selectAt[tick] then bot:SelectWeapon(selectAt[tick]) end
			local aw = bot:GetActiveWeapon()
			local row = { t = tick, active = IsValid(aw) and aw:GetClass() or "" }
			if IsValid(aw) then
				row.clip1 = aw:Clip1()
				row.clip2 = aw:Clip2()
				row.next_primary = math.Round(aw:GetNextPrimaryFire() - CurTime(), 4)
				row.next_secondary = math.Round(aw:GetNextSecondaryFire() - CurTime(), 4)
			end
			if tick == 0 or selectAt[tick] then row.ammo = ammoTable(bot) end
			ticks[#ticks + 1] = row
		end
		tick = tick + 1
		if tick >= total then
			hook.Remove("Tick", "parity_weapon")
			done({ loadout = loadout, ticks = ticks, ammo = ammoTable(bot), tickinterval = engine.TickInterval() })
		end
	end)
end
ASYNC.weapon = true


-- fire: a bot holding one weapon drives IN_ATTACK / IN_ATTACK2 / IN_RELOAD per segment; every
-- tick the clip, the reserve ammo, the fire timers and the bullets fired (EntityFireBullets)
-- are recorded, to measure fire rate, semi-auto behaviour, clip use and reload timing.
SCENARIOS.fire = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.fire(scn, done) end)
		return
	end
	local start = Vector(scn.start[1], scn.start[2], scn.start[3])
	local segs = scn.segments or {}
	local total = 0
	for _, sg in ipairs(segs) do total = total + (sg.ticks or 0) end
	local tick = -10
	local ticks, shots = {}, {}
	local shotCount, lastShot = 0, nil
	-- projectiles the weapon spawns (rocket, bolt, grenade, ball): tracked every tick until removed
	local PROJ = { rpg_missile = true, crossbow_bolt = true, npc_grenade_frag = true, prop_combine_ball = true, grenade_ar2 = true }
	local removed = {}
	hook.Add("EntityRemoved", "parity_fire", function(ent)
		if PROJ[ent:GetClass()] and tick >= 0 then
			local p = ent:GetPos()
			removed[#removed + 1] = { t = tick, class = ent:GetClass(), id = ent:EntIndex(), pos = { p.x, p.y, p.z } }
		end
	end)
	local function segAt(t)
		local acc = 0
		for _, sg in ipairs(segs) do
			acc = acc + (sg.ticks or 0)
			if t < acc then return sg end
		end
	end
	hook.Add("EntityFireBullets", "parity_fire", function(ent, data)
		if ent == bot then
			shotCount = shotCount + 1
			lastShot = { num = data.Num, spread = { data.Spread.x, data.Spread.y, data.Spread.z }, damage = data.Damage, force = data.Force,
				tracer = data.Tracer, tracername = data.TracerName, ammo = data.AmmoType, dir = { data.Dir.x, data.Dir.y, data.Dir.z },
				src = { data.Src.x, data.Src.y, data.Src.z }, dist = data.Distance }
		end
	end)
	hook.Add("StartCommand", "parity_fire", function(ply, cmd)
		if ply ~= bot then return end
		cmd:ClearButtons()
		cmd:ClearMovement()
		cmd:SetViewAngles(Angle(scn.pitch or 0, scn.yaw or 90, 0))
		local sg = tick >= 0 and segAt(tick) or nil
		if sg then cmd:SetButtons(sg.buttons or 0) end
	end)
	hook.Add("Tick", "parity_fire", function()
		if tick == -10 then
			-- a fresh loadout: clips and ammo otherwise carry over between scenarios
			bot:StripWeapons()
			bot:RemoveAllAmmo()
			bot:Spawn()
			bot:SetPos(start)
			bot:SetEyeAngles(Angle(scn.pitch or 0, scn.yaw or 90, 0))
		end
		if tick == -8 then bot:SelectWeapon(scn.weapon) end
		if tick >= 0 then
			local aw = bot:GetActiveWeapon()
			local row = { t = tick, active = IsValid(aw) and aw:GetClass() or "", shots = shotCount }
			if IsValid(aw) then
				row.clip1 = aw:Clip1()
				row.clip2 = aw:Clip2()
				row.ammo1 = aw:GetPrimaryAmmoType() >= 0 and bot:GetAmmoCount(aw:GetPrimaryAmmoType()) or -1
				row.ammo2 = aw:GetSecondaryAmmoType() >= 0 and bot:GetAmmoCount(aw:GetSecondaryAmmoType()) or -1
				row.next_primary = math.Round(aw:GetNextPrimaryFire() - CurTime(), 4)
				row.next_secondary = math.Round(aw:GetNextSecondaryFire() - CurTime(), 4)
				row.sequence = aw:GetSequence()
				row.cycle = math.Round(aw:GetCycle(), 4)
			end
			if lastShot then row.shot = lastShot; lastShot = nil end
			local projs = {}
			for cls in pairs(PROJ) do
				for _, e in ipairs(ents.FindByClass(cls)) do
					local p, a = e:GetPos(), e:GetAngles()
					local v = e:GetVelocity()
					local ph = e:GetPhysicsObject()
					if IsValid(ph) then v = ph:GetVelocity() end
					projs[#projs + 1] = { class = cls, id = e:EntIndex(), pos = { p.x, p.y, p.z }, vel = { v.x, v.y, v.z }, ang = { a.p, a.y, a.r },
						movetype = e:GetMoveType(), phys = IsValid(ph), owner = IsValid(e:GetOwner()) and e:GetOwner():EntIndex() or -1,
						created = math.Round(CurTime() - e:GetCreationTime(), 4) }
				end
			end
			if #projs > 0 then row.projectiles = projs end
			ticks[#ticks + 1] = row
		end
		tick = tick + 1
		if tick >= total then
			hook.Remove("Tick", "parity_fire")
			hook.Remove("StartCommand", "parity_fire")
			hook.Remove("EntityFireBullets", "parity_fire")
			hook.Remove("EntityRemoved", "parity_fire")
			done({ weapon = scn.weapon, ticks = ticks, shots = shotCount, removed = removed, tickinterval = engine.TickInterval() })
		end
	end)
end
ASYNC.fire = true


-- hit: two bots; the shooter fires one weapon at the target standing `dist` units ahead; every
-- EntityTakeDamage / ScalePlayerDamage on the target is recorded with the target's health and
-- armour per tick, plus the sk_* damage convars, to measure the damage per bullet and hitgroup.
SCENARIOS.hit = function(scn, done)
	local bots = player.GetBots()
	if #bots < 2 then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.hit(scn, done) end)
		return
	end
	local bot, target = bots[1], bots[2]
	local start = Vector(scn.start[1], scn.start[2], scn.start[3])
	local dist = scn.dist or 200
	local tpos = start + Vector(0, dist, 0)
	local segs = scn.segments or {}
	local total = 0
	for _, sg in ipairs(segs) do total = total + (sg.ticks or 0) end
	local tick = -10
	local ticks, hits, scales = {}, {}, {}
	local shotCount = 0
	local function segAt(t)
		local acc = 0
		for _, sg in ipairs(segs) do
			acc = acc + (sg.ticks or 0)
			if t < acc then return sg end
		end
	end
	hook.Add("EntityFireBullets", "parity_hit", function(ent, data)
		if ent == bot then shotCount = shotCount + 1 end
	end)
	hook.Add("ScalePlayerDamage", "parity_hit", function(ply, hitgroup, dmg)
		if ply == target and tick >= 0 then
			scales[#scales + 1] = { t = tick, hitgroup = hitgroup, damage = dmg:GetDamage(), type = dmg:GetDamageType() }
		end
	end)
	hook.Add("EntityTakeDamage", "parity_hit", function(ent, dmg)
		if ent == target and tick >= 0 then
			local inf, att = dmg:GetInflictor(), dmg:GetAttacker()
			local pos = dmg:GetDamagePosition()
			hits[#hits + 1] = { t = tick, damage = dmg:GetDamage(), base = dmg:GetBaseDamage(), type = dmg:GetDamageType(),
				inflictor = IsValid(inf) and inf:GetClass() or "", attacker = IsValid(att) and att:GetClass() or "",
				ammo = dmg:GetAmmoType(), pos = { pos.x, pos.y, pos.z }, force = dmg:GetDamageForce():Length(),
				health_before = target:Health(), armor_before = target:Armor(), shots = shotCount }
		end
	end)
	hook.Add("StartCommand", "parity_hit", function(ply, cmd)
		if ply ~= bot and ply ~= target then return end
		cmd:ClearButtons()
		cmd:ClearMovement()
		if ply == target then
			cmd:SetViewAngles(Angle(0, -90, 0))
			return
		end
		cmd:SetViewAngles(Angle(scn.pitch or 0, scn.yaw or 90, 0))
		local sg = tick >= 0 and segAt(tick) or nil
		if sg then cmd:SetButtons(sg.buttons or 0) end
	end)
	hook.Add("Tick", "parity_hit", function()
		if tick == -10 then
			for _, b in ipairs({ bot, target }) do
				b:StripWeapons()
				b:RemoveAllAmmo()
				b:Spawn()
			end
			bot:SetPos(start)
			bot:SetEyeAngles(Angle(scn.pitch or 0, scn.yaw or 90, 0))
			target:SetPos(tpos)
			target:SetEyeAngles(Angle(0, -90, 0))
			if scn.target_health then target:SetHealth(scn.target_health) end
			if scn.target_armor then target:SetArmor(scn.target_armor) end
		end
		if tick == -8 then bot:SelectWeapon(scn.weapon) end
		if tick >= 0 then
			local p = target:GetPos()
			ticks[#ticks + 1] = { t = tick, health = target:Health(), armor = target:Armor(), alive = target:Alive(), shots = shotCount,
				target_pos = { p.x, p.y, p.z } }
		end
		tick = tick + 1
		if tick >= total then
			for _, h in ipairs({ "Tick", "StartCommand", "EntityFireBullets", "EntityTakeDamage", "ScalePlayerDamage" }) do hook.Remove(h, "parity_hit") end
			local cv = {}
			for _, n in ipairs({ "sk_plr_dmg_pistol", "sk_plr_dmg_smg1", "sk_plr_dmg_ar2", "sk_plr_dmg_357", "sk_plr_dmg_buckshot", "sk_plr_dmg_crossbow",
				"sk_plr_dmg_rpg_round", "sk_plr_dmg_grenade", "sk_plr_dmg_smg1_grenade", "sk_plr_dmg_crowbar", "sk_npc_dmg_pistol", "sk_npc_dmg_smg1",
				"sk_npc_dmg_buckshot", "sk_npc_dmg_357", "sk_max_pistol", "sk_max_smg1", "sk_max_buckshot", "sk_max_357", "sk_max_ar2",
				"sbox_playershurtplayers", "mp_friendlyfire", "sv_cheats" }) do
				local c = GetConVar(n)
				cv[n] = c and c:GetString() or nil
			end
			local eye = bot:GetShootPos()
			done({ weapon = scn.weapon, dist = dist, ticks = ticks, hits = hits, scales = scales, shots = shotCount, convars = cv,
				eye = { eye.x, eye.y, eye.z }, target_mins = { target:OBBMins().x, target:OBBMins().y, target:OBBMins().z },
				target_maxs = { target:OBBMaxs().x, target:OBBMaxs().y, target:OBBMaxs().z }, target_model = target:GetModel() })
		end
	end)
end
ASYNC.hit = true


-- damage: a bot dropped from the scenario's heights (fall damage) and hit with TakeDamage of
-- given amounts at given armour, recorded every tick with the damage events, to measure the
-- fall damage rule, armour absorption and death.
SCENARIOS.damage = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.damage(scn, done) end)
		return
	end
	if scn.falldamage ~= nil then RunConsoleCommand("mp_falldamage", tostring(scn.falldamage)) end
	if not bot:Alive() then bot:Spawn() end
	local base = Vector(scn.start[1], scn.start[2], scn.start[3])
	local drops = {}
	for i, h in ipairs(scn.heights or {}) do drops[(i - 1) * (scn.spacing or 200)] = h end
	local hits = {}
	for _, h in ipairs(scn.hits or {}) do hits[h.t] = h end
	local total = scn.ticks or 600
	local tick = -5
	local ticks, events = {}, {}
	hook.Add("EntityTakeDamage", "parity_damage", function(ent, dmg)
		if ent ~= bot then return end
		events[#events + 1] = { t = tick, event = "EntityTakeDamage", amount = dmg:GetDamage(), type = dmg:GetDamageType(),
			health = ent:Health(), armor = ent:Armor() }
	end)
	hook.Add("PlayerHurt", "parity_damage", function(ply, attacker, remaining, taken)
		if ply ~= bot then return end
		events[#events + 1] = { t = tick, event = "PlayerHurt", taken = taken, remaining = remaining, armor = ply:Armor() }
	end)
	hook.Add("PlayerDeath", "parity_damage", function(ply) if ply == bot then events[#events + 1] = { t = tick, event = "PlayerDeath" } end end)
	hook.Add("PlayerSpawn", "parity_damage", function(ply) if ply == bot then events[#events + 1] = { t = tick, event = "PlayerSpawn" } end end)
	hook.Add("StartCommand", "parity_damage", function(ply, cmd)
		if ply ~= bot then return end
		cmd:ClearButtons()
		cmd:ClearMovement()
	end)
	hook.Add("Tick", "parity_damage", function()
		if not IsValid(bot) then return end
		if tick >= 0 then
			local h = drops[tick]
			if h then
				if not bot:Alive() then bot:Spawn() end
				bot:SetHealth(scn.health or 100)
				bot:SetArmor(scn.armor or 0)
				bot:SetPos(base + Vector(0, 0, h))
				bot:SetLocalVelocity(Vector(0, 0, 0))
				events[#events + 1] = { t = tick, event = "drop", height = h }
			end
			local hit = hits[tick]
			if hit then
				if hit.armor then bot:SetArmor(hit.armor) end
				if hit.health then bot:SetHealth(hit.health) end
				local d = DamageInfo()
				d:SetDamage(hit.amount)
				d:SetAttacker(game.GetWorld())
				d:SetInflictor(game.GetWorld())
				d:SetDamageType(hit.type or DMG_GENERIC)
				bot:TakeDamageInfo(d)
				events[#events + 1] = { t = tick, event = "hit", amount = hit.amount, type = hit.type or DMG_GENERIC }
			end
			local p, v = bot:GetPos(), bot:GetVelocity()
			ticks[#ticks + 1] = { t = tick, pos = { p.x, p.y, p.z }, vel = { v.x, v.y, v.z }, health = bot:Health(), armor = bot:Armor(),
				alive = bot:Alive(), ground = bot:IsOnGround() }
		end
		tick = tick + 1
		if tick >= total then
			for _, hk in ipairs({ "EntityTakeDamage", "PlayerHurt", "PlayerDeath", "PlayerSpawn", "StartCommand", "Tick" }) do hook.Remove(hk, "parity_damage") end
			done({ ticks = ticks, events = events, falldamage = GetConVar("mp_falldamage"):GetInt() })
		end
	end)
end
ASYNC.damage = true


-- ragdoll: the bot is killed standing still; every physics object of its ragdoll (position,
-- angles, velocity) is recorded per tick, with the bone names and masses, to measure how the
-- player ragdoll is built and how it falls.
SCENARIOS.ragdoll = function(scn, done)
	local bot = player.GetBots()[1]
	if not IsValid(bot) then
		RunConsoleCommand("bot")
		timer.Simple(1, function() SCENARIOS.ragdoll(scn, done) end)
		return
	end
	if not bot:Alive() then bot:Spawn() end
	local start = Vector(scn.start[1], scn.start[2], scn.start[3])
	local ang = Angle(0, scn.yaw or 0, 0)
	bot:SetPos(start)
	bot:SetEyeAngles(ang)
	bot:SetLocalVelocity(Vector(0, 0, 0))
	local total = scn.ticks or 300
	local killAt = type(scn.kill_at) == "table" and scn.kill_at[1] or scn.kill_at or 20
	local tick = -10
	local ticks, info = {}, nil
	hook.Add("StartCommand", "parity_ragdoll", function(ply, cmd)
		if ply ~= bot then return end
		cmd:ClearButtons()
		cmd:ClearMovement()
		cmd:SetViewAngles(ang)
	end)
	hook.Add("Tick", "parity_ragdoll", function()
		if not IsValid(bot) then return end
		if tick >= 0 then
			if tick == killAt then
				bot:Kill()
			end
			local row = { t = tick }
			local p, v = bot:GetPos(), bot:GetVelocity()
			row.pos = { p.x, p.y, p.z }
			row.vel = { v.x, v.y, v.z }
			row.alive = bot:Alive()
			local rag = bot:GetRagdollEntity()
			if IsValid(rag) then
				local n = rag:GetPhysicsObjectCount()
				if not info then
					info = { model = rag:GetModel(), class = rag:GetClass(), count = n, bones = {} }
					for i = 0, n - 1 do
						local po = rag:GetPhysicsObjectNum(i)
						local bone = rag:TranslatePhysBoneToBone(i)
						info.bones[i + 1] = { bone = bone, name = rag:GetBoneName(bone), mass = po:GetMass(),
							material = po:GetMaterial(), surfacearea = po:GetSurfaceArea(), volume = po:GetVolume() }
					end
					local rp, ra = rag:GetPos(), rag:GetAngles()
					info.origin = { rp.x, rp.y, rp.z }
					info.angles = { ra.p, ra.y, ra.r }
					info.tick = tick
				end
				row.objects = {}
				for i = 0, n - 1 do
					local po = rag:GetPhysicsObjectNum(i)
					local pp, pa, pv, pw = po:GetPos(), po:GetAngles(), po:GetVelocity(), po:GetAngleVelocity()
					row.objects[i + 1] = { pos = { pp.x, pp.y, pp.z }, ang = { pa.p, pa.y, pa.r }, vel = { pv.x, pv.y, pv.z },
						angvel = { pw.x, pw.y, pw.z }, asleep = po:IsAsleep() }
				end
			end
			ticks[#ticks + 1] = row
		end
		tick = tick + 1
		if tick >= total then
			hook.Remove("StartCommand", "parity_ragdoll")
			hook.Remove("Tick", "parity_ragdoll")
			done({ ticks = ticks, ragdoll = info })
		end
	end)
end
ASYNC.ragdoll = true

local function runAll()
	file.CreateDir(OUT_DIR)
	local files = file.Find(SCN_DIR .. "/*.json", "DATA")
	table.sort(files or {})
	local quit = false
	local i = 0
	local function finish()
		file.Write(OUT_DIR .. "/_done.json", util.TableToJSON({ done = true, count = #(files or {}) }, false))
		if quit then
			timer.Simple(1, function() game.ConsoleCommand("quit\n") end)
		end
	end
	local function writeTrace(f, scn, ok, res, t0)
		local trace = {
			scenario = scn.name or f,
			kind = scn.kind,
			ok = ok,
			result = ok and res or nil,
			error = (not ok) and tostring(res) or nil,
			seconds = SysTime() - t0,
			game = { version = VERSION, branch = BRANCH, map = game.GetMap() },
		}
		file.Write(OUT_DIR .. "/" .. string.gsub(f, "%.json$", "") .. ".json", util.TableToJSON(trace, false))
		log("%s: %s (%.2fs)", f, ok and "ok" or ("error: " .. tostring(res)), SysTime() - t0)
	end
	local function next()
		i = i + 1
		local f = files and files[i]
		if not f then return finish() end
		local path = SCN_DIR .. "/" .. f
		local scn = util.JSONToTable(file.Read(path, "DATA") or "")
		if not scn then
			log("bad scenario %s", path)
			return next()
		end
		if scn.kind == "quit" then
			quit = true
			return next()
		end
		local fn = SCENARIOS[scn.kind]
		if not fn then
			log("unknown kind %s in %s", tostring(scn.kind), path)
			return next()
		end
		local t0 = SysTime()
		if ASYNC[scn.kind] then
			local ok, err = pcall(fn, scn, function(res)
				writeTrace(f, scn, true, res, t0)
				timer.Simple(0.1, next)
			end)
			if not ok then
				writeTrace(f, scn, false, err, t0)
				return next()
			end
		else
			local ok, res = pcall(fn, scn)
			writeTrace(f, scn, ok, res, t0)
			return next()
		end
	end
	next()
end

hook.Add("InitPostEntity", "parity_probe", function()
	timer.Simple(2, runAll)
end)
