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
local ASYNC = { move = true }

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
