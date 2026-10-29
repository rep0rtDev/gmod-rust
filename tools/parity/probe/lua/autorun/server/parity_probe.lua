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

local function runAll()
	file.CreateDir(OUT_DIR)
	local files = file.Find(SCN_DIR .. "/*.json", "DATA")
	local quit = false
	for _, f in ipairs(files or {}) do
		local path = SCN_DIR .. "/" .. f
		local scn = util.JSONToTable(file.Read(path, "DATA") or "")
		if not scn then
			log("bad scenario %s", path)
		elseif scn.kind == "quit" then
			quit = true
		else
			local fn = SCENARIOS[scn.kind]
			if not fn then
				log("unknown kind %s in %s", tostring(scn.kind), path)
			else
				local t0 = SysTime()
				local ok, res = pcall(fn, scn)
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
		end
	end
	file.Write(OUT_DIR .. "/_done.json", util.TableToJSON({ done = true, count = #(files or {}) }, false))
	if quit then
		timer.Simple(1, function() game.ConsoleCommand("quit\n") end)
	end
end

hook.Add("InitPostEntity", "parity_probe", function()
	timer.Simple(2, runAll)
end)
