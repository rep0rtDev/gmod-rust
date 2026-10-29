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
