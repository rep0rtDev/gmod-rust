-- GLua client drawing surface over the host's `draw.*` command list (own implementation from
-- the documented behaviour): `surface`, `draw`, `cam` (3D2D), `ScrW` / `ScrH`, `Material`, and
-- the HUD hook chain the game runs each frame (`PreDrawHUD`, `HUDShouldDraw`,
-- `HUDPaintBackground`, `HUDPaint`, `PostDrawHUD`, `DrawOverlay`). On the server realm the
-- same names exist as stubs (documented, not available there).
local stub = __glua_stub
local hostdraw = draw

if not CLIENT then
	for _, n in ipairs({ "SetDrawColor", "GetDrawColor", "DrawRect", "DrawOutlinedRect", "DrawLine", "SetFont", "SetTextColor", "GetTextColor", "SetTextPos", "GetTextPos", "DrawText", "GetTextSize", "CreateFont", "SetMaterial", "SetTexture", "GetTextureID", "DrawTexturedRect", "DrawTexturedRectUV", "DrawTexturedRectRotated", "DrawPoly", "DrawCircle", "ScreenWidth", "ScreenHeight", "PlaySound", "SetAlphaMultiplier", "GetAlphaMultiplier", "DisableClipping", "GetHUDTexture" }) do
		surface = surface or {}
		surface[n] = stub("surface." .. n)
	end
	draw = draw or {}
	for _, n in ipairs({ "SimpleText", "SimpleTextOutlined", "DrawText", "RoundedBox", "RoundedBoxEx", "WordBox", "Text", "TextShadow", "NoTexture", "GetFontHeight", "TexturedQuad" }) do draw[n] = stub("draw." .. n) end
	cam = cam or {}
	for _, n in ipairs({ "Start2D", "End2D", "Start3D", "End3D", "Start3D2D", "End3D2D", "IgnoreZ", "Start", "End", "PushModelMatrix", "PopModelMatrix", "StartOrthoView", "EndOrthoView" }) do cam[n] = stub("cam." .. n) end
	input = input or {}
	for _, n in ipairs({ "CheckKeyTrapping", "GetAnalogValue", "GetCursorPos", "GetKeyCode", "GetKeyName", "IsButtonDown", "IsControlDown", "IsKeyDown", "IsKeyTrapping", "IsMouseDown", "IsShiftDown", "LookupBinding", "LookupKeyBinding", "SelectWeapon", "SetCursorPos", "StartKeyTrapping", "TranslateAlias", "WasKeyPressed", "WasKeyReleased", "WasKeyTyped", "WasMouseDoublePressed", "WasMousePressed", "WasMouseReleased" }) do input[n] = stub("input." .. n) end
	gui = gui or {}
	for _, n in ipairs({ "ActivateGameUI", "AddCaption", "EnableScreenClicker", "HideGameUI", "InternalCursorMoved", "InternalKeyCodePressed", "InternalKeyCodeReleased", "InternalKeyCodeTyped", "InternalKeyTyped", "InternalMouseDoublePressed", "InternalMousePressed", "InternalMouseReleased", "InternalMouseWheeled", "IsConsoleVisible", "IsGameUIVisible", "MousePos", "MouseX", "MouseY", "OpenURL", "ScreenToVector", "SetMousePos", "ShowConsole" }) do gui[n] = stub("gui." .. n) end
	killicon = killicon or {}
	for _, n in ipairs({ "Add", "AddAlias", "AddFont", "AddTexCoord", "Draw", "Exists", "GetSize", "Render" }) do killicon[n] = stub("killicon." .. n) end
	dragndrop = dragndrop or {}
	for _, n in ipairs({ "CallReceiverFunction", "Clear", "Drop", "GetDroppable", "HandleDroppedInGame", "HoverThink", "IsDragging", "StartDragging", "StopDragging", "Think", "UpdateReceiver" }) do dragndrop[n] = stub("dragndrop." .. n) end
	spawnmenu = spawnmenu or {}
	for _, n in ipairs({ "ActivateTool", "ActivateToolPanel", "ActiveControlPanel", "AddContentType", "AddCreationTab", "AddPropCategory", "AddToolCategory", "AddToolMenuOption", "AddToolTab", "ClearToolMenus", "CreateContentIcon", "DoSaveToTextFiles", "GetContentType", "GetCreationTabs", "GetCustomPropTable", "GetPropTable", "GetToolMenu", "GetTools", "PopulateFromEngineTextFiles", "PopulateFromTextFiles", "SaveToTextFiles", "SetActiveControlPanel", "SwitchCreationTab", "SwitchToolTab" }) do spawnmenu[n] = stub("spawnmenu." .. n) end
	achievements = achievements or {}
	for _, n in ipairs({ "BalloonPopped", "Count", "EatBall", "GetCount", "GetDesc", "GetGoal", "GetName", "IncBaddies", "IncBystander", "IncGoodies", "IsAchieved", "Remover", "SpawnMenuOpen", "SpawnedNPC", "SpawnedProp", "SpawnedRagdoll" }) do achievements[n] = stub("achievements." .. n) end
	ScrW = stub("Global.ScrW")
	ScrH = stub("Global.ScrH")
	Material = stub("Global.Material")
	LocalPlayer = stub("Global.LocalPlayer")
	return
end

------------------------------------------------------------------------ the local player
-- `LocalPlayer()`: our own player entity once the server accepted us, NULL before that
-- (the game returns an invalid entity while connecting)
function LocalPlayer()
	local id = world and world.local_player()
	if id then return Player(id) end
	return NULL
end

------------------------------------------------------------------------ screen
local scr_w, scr_h = 1280, 720
function ScrW() return scr_w end
function ScrH() return scr_h end

TEXT_ALIGN_LEFT, TEXT_ALIGN_CENTER, TEXT_ALIGN_RIGHT, TEXT_ALIGN_TOP, TEXT_ALIGN_BOTTOM = 0, 1, 2, 3, 4

------------------------------------------------------------------------ materials
-- `Material(name)`: a handle naming a VMT; the client resolves `$basetexture` when a command
-- carrying it is drawn. Missing files draw the client's error texture (`IsError` true only
-- for an empty name here: the client reports load failures in its console).
local MAT = {}
MAT.__index = MAT
local materials = {}
function Material(name, params)
	name = tostring(name or "")
	local m = materials[name]
	if not m then
		m = setmetatable({ __name = name, __params = params or "" }, MAT)
		materials[name] = m
	end
	return m, 0
end
function MAT:GetName() return self.__name end
function MAT:IsError() return self.__name == "" end
function MAT:Width() return 0 end
function MAT:Height() return 0 end
function MAT:GetString(k) return nil end
function MAT:GetInt(k) return nil end
function MAT:GetFloat(k) return nil end
function MAT:GetVector(k) return nil end
function MAT:GetTexture(k) return nil end
function MAT:SetFloat() end
function MAT:SetInt() end
function MAT:SetString() end
function MAT:SetVector() end
function MAT:SetTexture() end
function MAT:Recompute() end
function MAT:IsValid() return true end

------------------------------------------------------------------------ surface
surface = surface or {}
local S = surface
local draw_color = { r = 255, g = 255, b = 255, a = 255 }
local text_color = { r = 255, g = 255, b = 255, a = 255 }
local text_x, text_y = 0, 0
local font = "DermaDefault"
local material = nil
local alpha_mul = 1
local fonts = {}
-- the vgui layer paints each panel in its own coordinates: `surface.__origin(x, y)` moves
-- the origin of every draw call to the panel's screen position
local ox, oy = 0, 0
function S.__origin(x, y) ox, oy = x or 0, y or 0 end
function S.__get_origin() return ox, oy end

local function col(c, mul)
	local a = (c.a or 255) * alpha_mul * (mul or 1)
	return { r = c.r or 255, g = c.g or 255, b = c.b or 255, a = a }
end
local function set_color(c, r, g, b, a)
	if type(r) == "table" then
		c.r, c.g, c.b, c.a = r.r or 255, r.g or 255, r.b or 255, r.a or 255
	else
		c.r, c.g, c.b, c.a = r or 255, g or 255, b or 255, a or 255
	end
end

function S.SetDrawColor(r, g, b, a) set_color(draw_color, r, g, b, a) end
function S.GetDrawColor() return Color(draw_color.r, draw_color.g, draw_color.b, draw_color.a) end
function S.SetTextColor(r, g, b, a) set_color(text_color, r, g, b, a) end
function S.GetTextColor() return Color(text_color.r, text_color.g, text_color.b, text_color.a) end
function S.SetTextPos(x, y) text_x, text_y = x, y end
function S.GetTextPos() return text_x, text_y end
function S.SetAlphaMultiplier(m) alpha_mul = m or 1 end
function S.GetAlphaMultiplier() return alpha_mul end
function S.ScreenWidth() return scr_w end
function S.ScreenHeight() return scr_h end
function S.DisableClipping(b) return false end
function S.PlaySound(path) end

function S.DrawRect(x, y, w, h) hostdraw.rect(x + ox, y + oy, w, h, col(draw_color)) end
function S.DrawOutlinedRect(x, y, w, h, t)
	t = t or 1
	local c = col(draw_color)
	x, y = x + ox, y + oy
	hostdraw.rect(x, y, w, t, c)
	hostdraw.rect(x, y + h - t, w, t, c)
	hostdraw.rect(x, y, t, h, c)
	hostdraw.rect(x + w - t, y, t, h, c)
end
-- a line is drawn as the thin rectangle between its ends; a diagonal one as a chain of
-- one-pixel steps (the client has no line primitive in the command list)
function S.DrawLine(x1, y1, x2, y2)
	local c = col(draw_color)
	x1, y1, x2, y2 = x1 + ox, y1 + oy, x2 + ox, y2 + oy
	if y1 == y2 then
		hostdraw.rect(math.min(x1, x2), y1, math.abs(x2 - x1) + 1, 1, c)
	elseif x1 == x2 then
		hostdraw.rect(x1, math.min(y1, y2), 1, math.abs(y2 - y1) + 1, c)
	else
		local n = math.max(math.abs(x2 - x1), math.abs(y2 - y1))
		for i = 0, n do
			local t = i / n
			hostdraw.rect(x1 + (x2 - x1) * t, y1 + (y2 - y1) * t, 1, 1, c)
		end
	end
end

-- `surface.CreateFont(name, { font, size, weight, ... })`: the host rasterises the family at
-- `size` (the game's size is in pixels of line height at 1080p); weight / italic / outline
-- are recorded for `GetFontHeight` and ignored by the rasteriser
function S.CreateFont(name, data)
	data = data or {}
	local size = tonumber(data.size) or 13
	fonts[name] = { family = data.font or "Tahoma", size = size, weight = data.weight or 500, data = data }
	hostdraw.font(name, { family = data.font or "Tahoma", size = size })
end
function S.SetFont(name) font = name or "DermaDefault" end
function S.GetTextSize(text)
	if fonts[font] == nil and font ~= "" then
		-- the game's default fonts exist without CreateFont; declare them at first use
		local defaults = { DermaDefault = 13, DermaDefaultBold = 13, DermaLarge = 32, Default = 13, Trebuchet18 = 18, Trebuchet24 = 24, HudHintTextLarge = 17, HudHintTextSmall = 13, CenterPrintText = 24, HudSelectionText = 11, ChatFont = 17, TargetID = 23, CloseCaption_Normal = 20, BudgetLabel = 14, DebugFixed = 13, DebugFixedSmall = 10 }
		local size = defaults[font]
		if size then S.CreateFont(font, { font = "Tahoma", size = size }) end
	end
	local w, h = hostdraw.text_size(tostring(text), font)
	return w, h
end
function S.DrawText(text)
	if fonts[font] == nil then S.GetTextSize("") end
	hostdraw.text(text_x + ox, text_y + oy, tostring(text), col(text_color), font)
	local w = hostdraw.text_size(tostring(text), font)
	text_x = text_x + (w or 0)
end

function S.SetMaterial(m) material = m end
function S.SetTexture(id) material = type(id) == "table" and id or nil end
function S.GetTextureID(name) return Material(name) end
function S.GetHUDTexture(name) return Material(name) end
function S.DrawTexturedRect(x, y, w, h)
	if material and material.__name ~= "" then
		hostdraw.texture(x + ox, y + oy, w, h, material.__name, col(draw_color))
	else
		hostdraw.rect(x + ox, y + oy, w, h, col(draw_color))
	end
end
function S.DrawTexturedRectUV(x, y, w, h) S.DrawTexturedRect(x, y, w, h) end
function S.DrawTexturedRectRotated(x, y, w, h, rot) S.DrawTexturedRect(x - w / 2, y - h / 2, w, h) end
-- a polygon is filled by horizontal scanlines (even-odd rule), one pixel tall
function S.DrawPoly(verts)
	local n = #verts
	if n < 3 then return end
	local ymin, ymax = math.huge, -math.huge
	for _, v in ipairs(verts) do ymin = math.min(ymin, v.y); ymax = math.max(ymax, v.y) end
	local c = col(draw_color)
	for y = math.floor(ymin), math.ceil(ymax) do
		local xs = {}
		for i = 1, n do
			local a, b = verts[i], verts[i % n + 1]
			if (a.y <= y and b.y > y) or (b.y <= y and a.y > y) then
				xs[#xs + 1] = a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x)
			end
		end
		table.sort(xs)
		for i = 1, #xs - 1, 2 do hostdraw.rect(xs[i] + ox, y + oy, xs[i + 1] - xs[i], 1, c) end
	end
end
function S.DrawCircle(x, y, r, c)
	if c then S.SetDrawColor(c) end
	local verts = {}
	local seg = math.max(12, math.floor(r))
	for i = 0, seg - 1 do
		local a = i / seg * math.pi * 2
		verts[#verts + 1] = { x = x + math.cos(a) * r, y = y + math.sin(a) * r }
	end
	S.DrawPoly(verts)
end

------------------------------------------------------------------------ draw (GLua)
draw = {}
local D = draw
function D.NoTexture() material = nil end
function D.GetFontHeight(f)
	local old = font
	S.SetFont(f)
	local _, h = S.GetTextSize("W")
	font = old
	return h
end
function D.SimpleText(text, f, x, y, color, xalign, yalign)
	text = tostring(text == nil and "" or text)
	S.SetFont(f or "DermaDefault")
	local w, h = S.GetTextSize(text)
	x, y = x or 0, y or 0
	if xalign == TEXT_ALIGN_CENTER then x = x - w / 2 elseif xalign == TEXT_ALIGN_RIGHT then x = x - w end
	if yalign == TEXT_ALIGN_CENTER then y = y - h / 2 elseif yalign == TEXT_ALIGN_BOTTOM then y = y - h end
	S.SetTextPos(math.ceil(x), math.ceil(y))
	S.SetTextColor(color or Color(255, 255, 255, 255))
	S.DrawText(text)
	return w, h
end
function D.SimpleTextOutlined(text, f, x, y, color, xalign, yalign, width, ocolor)
	local steps = (width or 1) * 2 / 3
	if steps < 1 then steps = 1 end
	for ox = -width, width, steps do
		for oy = -width, width, steps do
			D.SimpleText(text, f, x + ox, y + oy, ocolor, xalign, yalign)
		end
	end
	return D.SimpleText(text, f, x, y, color, xalign, yalign)
end
-- `draw.DrawText`: newlines start a new line, tabs advance to the next 50 px stop
function D.DrawText(text, f, x, y, color, xalign)
	text = tostring(text == nil and "" or text)
	f = f or "DermaDefault"
	S.SetFont(f)
	local _, lh = S.GetTextSize("W")
	local cy = y or 0
	for line in (text .. "\n"):gmatch("(.-)\n") do
		local cx = x or 0
		local first = true
		for seg in (line .. "\t"):gmatch("(.-)\t") do
			if not first then cx = math.ceil((cx - (x or 0)) / 50 + 0.0001) * 50 + (x or 0) end
			D.SimpleText(seg, f, cx, cy, color, first and xalign or TEXT_ALIGN_LEFT)
			cx = cx + (S.GetTextSize(seg))
			first = false
		end
		cy = cy + lh
	end
end
function D.Text(t)
	return D.SimpleText(t.text, t.font, t.pos and t.pos[1] or 0, t.pos and t.pos[2] or 0, t.color, t.xalign, t.yalign)
end
function D.TextShadow(t, dist, alpha)
	local c = t.color or Color(255, 255, 255, 255)
	local shadow = { text = t.text, font = t.font, pos = { (t.pos and t.pos[1] or 0) + (dist or 1), (t.pos and t.pos[2] or 0) + (dist or 1) }, color = Color(0, 0, 0, alpha or 200), xalign = t.xalign, yalign = t.yalign }
	D.Text(shadow)
	return D.Text(t)
end
-- rounded boxes: the corners are cut by `radius` rows of narrowing rectangles (the game's
-- corner textures at 8 / 16 px; the fill is identical, the arc approximated per row)
function D.RoundedBoxEx(radius, x, y, w, h, color, tl, tr, bl, br)
	local c = col(color or draw_color)
	radius = math.max(0, math.min(radius or 0, math.floor(math.min(w, h) / 2)))
	x, y = x + ox, y + oy
	if radius == 0 then hostdraw.rect(x, y, w, h, c) return end
	hostdraw.rect(x, y + radius, w, h - radius * 2, c)
	for i = 0, radius - 1 do
		local inset = radius - math.floor(math.sqrt(radius * radius - (radius - i - 0.5) ^ 2) + 0.5)
		local li, ri = tl and inset or 0, tr and inset or 0
		hostdraw.rect(x + li, y + i, w - li - ri, 1, c)
		local lb, rb = bl and inset or 0, br and inset or 0
		hostdraw.rect(x + lb, y + h - 1 - i, w - lb - rb, 1, c)
	end
end
function D.RoundedBox(radius, x, y, w, h, color) D.RoundedBoxEx(radius, x, y, w, h, color, true, true, true, true) end
function D.WordBox(bordersize, x, y, text, f, color, fontcolor, xalign, yalign)
	S.SetFont(f or "DermaDefault")
	local w, h = S.GetTextSize(tostring(text))
	local bw, bh = w + bordersize * 2, h + bordersize * 2
	if xalign == TEXT_ALIGN_CENTER then x = x - bw / 2 elseif xalign == TEXT_ALIGN_RIGHT then x = x - bw end
	if yalign == TEXT_ALIGN_CENTER then y = y - bh / 2 elseif yalign == TEXT_ALIGN_BOTTOM then y = y - bh end
	D.RoundedBox(bordersize, x, y, bw, bh, color)
	S.SetTextPos(x + bordersize, y + bordersize)
	S.SetTextColor(fontcolor or Color(255, 255, 255, 255))
	S.DrawText(text)
	return bw, bh
end
function D.TexturedQuad(t)
	S.SetTexture(t.texture)
	S.SetDrawColor(t.color or Color(255, 255, 255, 255))
	S.DrawTexturedRect(t.x, t.y, t.w, t.h)
end

------------------------------------------------------------------------ cam
cam = cam or {}
function cam.Start3D2D(pos, ang, scale) hostdraw.start3d2d(pos, ang, scale or 1) end
function cam.End3D2D() hostdraw.end3d2d() end
function cam.Start2D() end
function cam.End2D() end
function cam.Start3D() end
function cam.End3D() end
function cam.Start() end
function cam.End() end
function cam.IgnoreZ(b) end
function cam.PushModelMatrix() end
function cam.PopModelMatrix() end
function cam.StartOrthoView() end
function cam.EndOrthoView() end


------------------------------------------------------------------------ keys: names, codes, state
-- the client's physical key names (winit) -> the game's KEY_ / MOUSE_ codes, typed characters,
-- and the game's button names (`input.GetKeyName`: "a", "SPACE", "MOUSE1", ...)
local KEYNAMES = { Enter = "KEY_ENTER", NumpadEnter = "KEY_PAD_ENTER", Space = "KEY_SPACE", Backspace = "KEY_BACKSPACE", Tab = "KEY_TAB", Escape = "KEY_ESCAPE", Delete = "KEY_DELETE", Insert = "KEY_INSERT", Home = "KEY_HOME", End = "KEY_END", PageUp = "KEY_PAGEUP", PageDown = "KEY_PAGEDOWN", ArrowUp = "KEY_UP", ArrowDown = "KEY_DOWN", ArrowLeft = "KEY_LEFT", ArrowRight = "KEY_RIGHT", ShiftLeft = "KEY_LSHIFT", ShiftRight = "KEY_RSHIFT", ControlLeft = "KEY_LCONTROL", ControlRight = "KEY_RCONTROL", AltLeft = "KEY_LALT", AltRight = "KEY_RALT", CapsLock = "KEY_CAPSLOCK", NumLock = "KEY_NUMLOCK", ScrollLock = "KEY_SCROLLLOCK", Minus = "KEY_MINUS", Equal = "KEY_EQUAL", BracketLeft = "KEY_LBRACKET", BracketRight = "KEY_RBRACKET", Semicolon = "KEY_SEMICOLON", Quote = "KEY_APOSTROPHE", Backquote = "KEY_BACKQUOTE", Comma = "KEY_COMMA", Period = "KEY_PERIOD", Slash = "KEY_SLASH", Backslash = "KEY_BACKSLASH", NumpadDivide = "KEY_PAD_DIVIDE", NumpadMultiply = "KEY_PAD_MULTIPLY", NumpadSubtract = "KEY_PAD_MINUS", NumpadAdd = "KEY_PAD_PLUS", NumpadDecimal = "KEY_PAD_DECIMAL", SuperLeft = "KEY_LWIN", SuperRight = "KEY_RWIN", ContextMenu = "KEY_APP", MouseLeft = "MOUSE_LEFT", MouseRight = "MOUSE_RIGHT", MouseMiddle = "MOUSE_MIDDLE", MouseBack = "MOUSE_4", MouseForward = "MOUSE_5", WheelUp = "MOUSE_WHEEL_UP", WheelDown = "MOUSE_WHEEL_DOWN" }
local SHIFTED = { ["1"] = "!", ["2"] = "@", ["3"] = "#", ["4"] = "$", ["5"] = "%", ["6"] = "^", ["7"] = "&", ["8"] = "*", ["9"] = "(", ["0"] = ")", ["-"] = "_", ["="] = "+", ["["] = "{", ["]"] = "}", [";"] = ":", ["'"] = '"', ["`"] = "~", [","] = "<", ["."] = ">", ["/"] = "?", ["\\"] = "|" }
local PUNCT = { Minus = "-", Equal = "=", BracketLeft = "[", BracketRight = "]", Semicolon = ";", Quote = "'", Backquote = "`", Comma = ",", Period = ".", Slash = "/", Backslash = "\\", Space = " " }
-- the game's button names per KEY_ constant name (source's `ButtonCode_t` names)
local BUTTON_NAMES = { KEY_ENTER = "ENTER", KEY_PAD_ENTER = "KP_ENTER", KEY_SPACE = "SPACE", KEY_BACKSPACE = "BACKSPACE", KEY_TAB = "TAB", KEY_ESCAPE = "ESCAPE", KEY_DELETE = "DEL", KEY_INSERT = "INS", KEY_HOME = "HOME", KEY_END = "END", KEY_PAGEUP = "PGUP", KEY_PAGEDOWN = "PGDN", KEY_UP = "UPARROW", KEY_DOWN = "DOWNARROW", KEY_LEFT = "LEFTARROW", KEY_RIGHT = "RIGHTARROW", KEY_LSHIFT = "SHIFT", KEY_RSHIFT = "RSHIFT", KEY_LCONTROL = "CTRL", KEY_RCONTROL = "RCTRL", KEY_LALT = "ALT", KEY_RALT = "RALT", KEY_CAPSLOCK = "CAPSLOCK", KEY_NUMLOCK = "NUMLOCK", KEY_SCROLLLOCK = "SCROLLLOCK", KEY_MINUS = "-", KEY_EQUAL = "=", KEY_LBRACKET = "[", KEY_RBRACKET = "]", KEY_SEMICOLON = "SEMICOLON", KEY_APOSTROPHE = "'", KEY_BACKQUOTE = "`", KEY_COMMA = ",", KEY_PERIOD = ".", KEY_SLASH = "/", KEY_BACKSLASH = "\\", KEY_PAD_DIVIDE = "KP_SLASH", KEY_PAD_MULTIPLY = "KP_MULTIPLY", KEY_PAD_MINUS = "KP_MINUS", KEY_PAD_PLUS = "KP_PLUS", KEY_PAD_DECIMAL = "KP_DEL", KEY_LWIN = "LWIN", KEY_RWIN = "RWIN", KEY_APP = "APP", KEY_PAUSE = "PAUSE", MOUSE_LEFT = "MOUSE1", MOUSE_RIGHT = "MOUSE2", MOUSE_MIDDLE = "MOUSE3", MOUSE_4 = "MOUSE4", MOUSE_5 = "MOUSE5", MOUSE_WHEEL_UP = "MWHEELUP", MOUSE_WHEEL_DOWN = "MWHEELDOWN" }
for i = 0, 9 do BUTTON_NAMES["KEY_" .. i] = tostring(i); BUTTON_NAMES["KEY_PAD_" .. i] = "KP_INS" end
for i = 1, 12 do BUTTON_NAMES["KEY_F" .. i] = "F" .. i end
for c = string.byte("A"), string.byte("Z") do BUTTON_NAMES["KEY_" .. string.char(c)] = string.char(c):lower() end
local PAD_NAMES = { "KP_END", "KP_DOWNARROW", "KP_PGDN", "KP_LEFTARROW", "KP_5", "KP_RIGHTARROW", "KP_HOME", "KP_UPARROW", "KP_PGUP" }
for i = 1, 9 do BUTTON_NAMES["KEY_PAD_" .. i] = PAD_NAMES[i] end
local code_to_name, name_to_code = {}, {}
for const, bname in pairs(BUTTON_NAMES) do
	local code = _G[const]
	if code then code_to_name[code] = bname; name_to_code[bname:lower()] = code end
end
MOUSE_LEFT, MOUSE_RIGHT, MOUSE_MIDDLE, MOUSE_4, MOUSE_5, MOUSE_WHEEL_UP, MOUSE_WHEEL_DOWN = 107, 108, 109, 110, 111, 112, 113
for const, code in pairs({ MOUSE_LEFT = 107, MOUSE_RIGHT = 108, MOUSE_MIDDLE = 109, MOUSE_4 = 110, MOUSE_5 = 111, MOUSE_WHEEL_UP = 112, MOUSE_WHEEL_DOWN = 113 }) do code_to_name[code] = BUTTON_NAMES[const]; name_to_code[BUTTON_NAMES[const]:lower()] = code end
local shift_down = false
function __glua_key_code(name)
	local k = KEYNAMES[name]
	if k then return _G[k] end
	local letter = name:match("^Key(%a)$")
	if letter then return _G["KEY_" .. letter:upper()] end
	local digit = name:match("^Digit(%d)$")
	if digit then return _G["KEY_" .. digit] end
	local fkey = name:match("^F(%d+)$")
	if fkey then return _G["KEY_F" .. fkey] end
	local pad = name:match("^Numpad(%d)$")
	if pad then return _G["KEY_PAD_" .. pad] end
end
function __glua_key_char(name)
	local letter = name:match("^Key(%a)$")
	if letter then return shift_down and letter:upper() or letter:lower() end
	local digit = name:match("^Digit(%d)$")
	if digit then return shift_down and SHIFTED[digit] or digit end
	local pad = name:match("^Numpad(%d)$")
	if pad then return pad end
	local p = PUNCT[name]
	if p then return shift_down and SHIFTED[p] or p end
end

input = input or {}
local keys_down, pressed_this_frame, released_this_frame, typed_this_frame = {}, {}, {}, {}
local last_press_time, double_pressed = {}, {}
local trapping, trapped = false, nil
local cursor_mx, cursor_my = 0, 0
function input.IsKeyDown(code) return keys_down[code] == true end
function input.IsMouseDown(code) return keys_down[code] == true end
function input.IsButtonDown(code) return keys_down[code] == true end
function input.IsShiftDown() return keys_down[KEY_LSHIFT] == true or keys_down[KEY_RSHIFT] == true end
function input.IsControlDown() return keys_down[KEY_LCONTROL] == true or keys_down[KEY_RCONTROL] == true end
function input.WasKeyPressed(code) return pressed_this_frame[code] == true end
function input.WasKeyReleased(code) return released_this_frame[code] == true end
function input.WasKeyTyped(code) return typed_this_frame[code] == true end
function input.WasMousePressed(code) return pressed_this_frame[code] == true end
function input.WasMouseReleased(code) return released_this_frame[code] == true end
function input.WasMouseDoublePressed(code) return double_pressed[code] == true end
function input.GetKeyName(code) return code_to_name[code] end
function input.GetKeyCode(name) return name_to_code[tostring(name):lower()] or 0 end
function input.GetCursorPos() return cursor_mx, cursor_my end
function input.SetCursorPos(x, y) cursor_mx, cursor_my = x, y end
function input.GetAnalogValue(code) return 0 end
function input.SelectWeapon(wep) if wep and wep.GetClass then RunConsoleCommand("use", wep:GetClass()) end end
function input.StartKeyTrapping() trapping, trapped = true, nil end
function input.IsKeyTrapping() return trapping end
function input.CheckKeyTrapping() if trapped then local k = trapped; trapped, trapping = nil, false; return k end end
-- the game's default binds (config/config_default.cfg); the client's own config layer is not
-- scripted yet, so lookups answer from these
local DEFAULT_BINDS = { ["+attack"] = "MOUSE1", ["+attack2"] = "MOUSE2", ["+jump"] = "SPACE", ["+duck"] = "CTRL", ["+use"] = "e", ["+forward"] = "w", ["+back"] = "s", ["+moveleft"] = "a", ["+moveright"] = "d", ["+speed"] = "SHIFT", ["+walk"] = "ALT", ["+reload"] = "r", ["+menu"] = "q", ["+menu_context"] = "c", ["+zoom"] = "z", ["+voicerecord"] = "x", ["impulse 100"] = "f", ["gm_showhelp"] = "F1", ["gm_showteam"] = "F2", ["gm_showspare1"] = "F3", ["gm_showspare2"] = "F4", ["jpeg"] = "F5", ["toggleconsole"] = "`", ["messagemode"] = "y", ["messagemode2"] = "u", ["+showscores"] = "TAB", ["noclip"] = "v", ["undo"] = "z", ["invnext"] = "MWHEELDOWN", ["invprev"] = "MWHEELUP", ["lastinv"] = "q", ["slot1"] = "1", ["slot2"] = "2", ["slot3"] = "3", ["slot4"] = "4", ["slot5"] = "5", ["slot6"] = "6", ["phys_swap"] = "MOUSE3", ["cancelselect"] = "ESCAPE" }
function input.LookupBinding(cmd, strict)
	local k = DEFAULT_BINDS[cmd]
	if k then return k:lower() end
	if not strict then for c, key in pairs(DEFAULT_BINDS) do if c:find(cmd, 1, true) then return key:lower() end end end
end
function input.LookupKeyBinding(code)
	local name = code_to_name[code]
	if not name then return end
	for c, key in pairs(DEFAULT_BINDS) do if key:lower() == name:lower() then return c end end
end
function input.TranslateAlias(cmd) return cmd end

gui = gui or {}
local screen_clicker = false
function gui.MousePos() return cursor_mx, cursor_my end
function gui.MouseX() return cursor_mx end
function gui.MouseY() return cursor_my end
function gui.SetMousePos(x, y) cursor_mx, cursor_my = x, y end
function gui.EnableScreenClicker(b)
	screen_clicker = b == true
	-- the client shows / grabs the cursor from this command (DrawCmd::ScreenClicker)
	if hostdraw.screen_clicker then hostdraw.screen_clicker(screen_clicker) end
end
function gui.IsGameUIVisible() return false end
function gui.ActivateGameUI() end
function gui.HideGameUI() end
function gui.IsConsoleVisible() return false end
function gui.ShowConsole() end
function gui.OpenURL(url) if hostdraw.open_url then hostdraw.open_url(url) end end
function gui.AddCaption(text, duration) end
function gui.ScreenToVector(x, y)
	-- the ray through a screen pixel from the view: the client's view angles / fov via the
	-- local player's eye angles; a unit vector in world space
	local lp = LocalPlayer and LocalPlayer()
	local ang = lp and lp.EyeAngles and lp:EyeAngles() or Angle()
	local fov = lp and lp.GetFOV and lp:GetFOV() or 90
	local w, h = ScrW(), ScrH()
	local tan_half = math.tan(math.rad(fov) / 2)
	local nx, ny = (x / w * 2 - 1) * tan_half * (w / h), (1 - y / h * 2) * tan_half
	local fwd, right, up = ang:Forward(), ang:Right(), ang:Up()
	local v = fwd + right * nx + up * ny
	v:Normalize()
	return v
end
function gui.InternalCursorMoved(x, y) cursor_mx, cursor_my = x, y end
function gui.InternalKeyCodePressed(code) keys_down[code] = true; pressed_this_frame[code] = true end
function gui.InternalKeyCodeReleased(code) keys_down[code] = nil; released_this_frame[code] = true end
function gui.InternalKeyCodeTyped(code) typed_this_frame[code] = true end
function gui.InternalKeyTyped(ch) end
function gui.InternalMousePressed(code) gui.InternalKeyCodePressed(code) end
function gui.InternalMouseReleased(code) gui.InternalKeyCodeReleased(code) end
function gui.InternalMouseDoublePressed(code) double_pressed[code] = true end
function gui.InternalMouseWheeled(d) end

-- every key edge updates the state before the vgui layer and the hooks see it
function __glua_key_edge(name, down)
	if name == "ShiftLeft" or name == "ShiftRight" then shift_down = down end
	local code = __glua_key_code(name)
	if not code then return end
	if down then
		if trapping and code ~= MOUSE_LEFT then trapped = code end
		keys_down[code] = true
		pressed_this_frame[code] = true
		typed_this_frame[code] = true
		local now = SysTime()
		if last_press_time[code] and now - last_press_time[code] < 0.3 then double_pressed[code] = true end
		last_press_time[code] = now
	else
		keys_down[code] = nil
		released_this_frame[code] = true
	end
	return code
end
function __glua_input_frame(p)
	pressed_this_frame, released_this_frame, typed_this_frame, double_pressed = {}, {}, {}, {}
	if p.mx then cursor_mx, cursor_my = p.mx, p.my end
end

------------------------------------------------------------------------ killicon
killicon = killicon or {}
local icons, icon_aliases = {}, {}
function killicon.Add(class, material, color) icons[class] = { material = material, color = color or Color(255, 80, 0, 255) } end
function killicon.AddFont(class, font, symbol, color) icons[class] = { font = font, symbol = symbol, color = color or Color(255, 80, 0, 255) } end
function killicon.AddTexCoord(class, material, color, x, y, w, h) icons[class] = { material = material, color = color or Color(255, 80, 0, 255), tex = { x, y, w, h } } end
function killicon.AddAlias(class, alias) icon_aliases[class] = alias end
local function icon_of(class) return icons[class] or (icon_aliases[class] and icons[icon_aliases[class]]) or icons.default end
function killicon.Exists(class) return icon_of(class) ~= nil end
function killicon.GetSize(class)
	local ic = icon_of(class)
	if not ic then return 0, 0 end
	if ic.font then surface.SetFont(ic.font); return surface.GetTextSize(ic.symbol) end
	return ic.tex and ic.tex[3] or 64, ic.tex and ic.tex[4] or 32
end
function killicon.Render(x, y, class, alpha)
	local ic = icon_of(class)
	if not ic then return end
	local c = Color(ic.color.r, ic.color.g, ic.color.b, alpha or 255)
	if ic.font then
		surface.SetFont(ic.font)
		surface.SetTextColor(c)
		surface.SetTextPos(x, y)
		surface.DrawText(ic.symbol)
	else
		local w, h = killicon.GetSize(class)
		surface.SetMaterial(Material(ic.material))
		surface.SetDrawColor(c)
		surface.DrawTexturedRect(x, y, w, h)
	end
end
function killicon.Draw(x, y, class, alpha)
	local w, h = killicon.GetSize(class)
	killicon.Render(x - w / 2, y - h / 2, class, alpha)
end
-- the game's icons from `lua/includes/extensions/client/killicon.lua`: the HL2 weapon glyphs
-- in the HL2MPTypeDeath font and the generic death icon
killicon.AddFont("default", "HL2MPTypeDeath", "0")
for class, symbol in pairs({ weapon_pistol = "-", weapon_357 = ".", weapon_smg1 = "/", weapon_ar2 = "2", weapon_shotgun = "0", weapon_crossbow = "1", weapon_frag = "4", weapon_rpg = "3", weapon_crowbar = "6", weapon_stunstick = "!", weapon_physcannon = ",", weapon_physgun = ",", prop_physics = "9", worldspawn = "6", npc_grenade_frag = "4", grenade_ar2 = "2", rpg_missile = "3", crossbow_bolt = "1", prop_combine_ball = "8", env_explosion = "*" }) do killicon.AddFont(class, "HL2MPTypeDeath", symbol) end

------------------------------------------------------------------------ dragndrop
dragndrop = dragndrop or {}
local drag = { panels = nil, name = nil, hover = nil, droppable = nil, start = nil, moved = false }
function dragndrop.IsDragging() return drag.panels ~= nil end
function dragndrop.GetDroppable(name) if not drag.panels then return end if name and drag.name ~= name then return end return drag.panels end
function dragndrop.StartDragging() drag.moved = true end
function dragndrop.StopDragging() drag.panels, drag.name, drag.hover, drag.droppable, drag.start, drag.moved = nil, nil, nil, nil, nil, false end
function dragndrop.Clear() dragndrop.StopDragging() end
function dragndrop.Think() end
function dragndrop.HoverThink() end
function dragndrop.UpdateReceiver() end
function dragndrop.CallReceiverFunction(receiver, dropped, cmd, x, y)
	local rc = receiver.__receivers and receiver.__receivers[drag.name]
	if rc then rc.fn(receiver, drag.panels, dropped, cmd, x, y) end
end
function dragndrop.Drop() if drag.panels then drag.panels = nil end end
function dragndrop.HandleDroppedInGame() end
-- Panel:Droppable(name) starts a drag of the panel when the mouse moves with the button down
-- on it; Panel:Receiver(name, fn, menu) takes the drop: fn(receiver, panels, dropped, cmd, x, y)
__glua_dnd = drag

------------------------------------------------------------------------ the frame
-- the engine HUD elements the game asks `HUDShouldDraw` about (the sandbox hides CHudHealth,
-- CHudBattery, CHudAmmo, CHudSecondaryAmmo and draws its own; the client applies the answers)
HUD_ELEMENTS = { "CHudHealth", "CHudBattery", "CHudAmmo", "CHudSecondaryAmmo", "CHudCrosshair", "CHudWeaponSelection", "CHudDamageIndicator", "CHudChat", "CHudDeathNotice", "CHudHintDisplay", "CHudZoom", "CHudHistoryResource", "CHudPoisonDamageIndicator", "CHudSquadStatus", "CHudTrain", "CHudVehicle", "CHudWeapon", "CHudGeiger", "CHudCloseCaption", "CHudMenu", "CHudMessage", "CHudSuitPower", "CHudCredits", "CHudVoiceStatus", "CHudVoiceSelfStatus", "CHUDQuickInfo", "CHudAnimationInfo", "CHudPredictionDump", "NetGraph" }
-- the game's HUD frame: `PreDrawHUD`, then `HUDPaintBackground` and `HUDPaint` unless
-- `HUDShouldDraw("CHudGMod")` vetoes, then `PostDrawHUD` and `DrawOverlay`; each hook runs
-- with the surface state reset (white, DermaDefault, no material)
if events then
	-- `Think` / `Tick` run once per client frame (the game calls the client's Think every frame)
	events.on("tick.client", function() hook.Run("Think"); hook.Run("Tick") end)
	events.on("render.hud", function(p)
		__glua_input_frame(p)
		local w, h = p.w or scr_w, p.h or scr_h
		if w ~= scr_w or h ~= scr_h then
			local ow, oh = scr_w, scr_h
			scr_w, scr_h = w, h
			hook.Run("OnScreenSizeChanged", ow, oh, w, h)
		end
		set_color(draw_color, 255, 255, 255, 255)
		set_color(text_color, 255, 255, 255, 255)
		font, material, alpha_mul = "DermaDefault", nil, 1
		ox, oy = 0, 0
		hook.Run("PreDrawHUD")
		-- the engine's HUD elements each ask `HUDShouldDraw(name)`; a false hides that element
		for _, name in ipairs(HUD_ELEMENTS) do
			if hook.Run("HUDShouldDraw", name) == false then hostdraw.hide_hud(name) end
		end
		if hook.Run("HUDShouldDraw", "CHudGMod") ~= false then
			hook.Run("HUDPaintBackground")
			hook.Run("HUDPaint")
		end
		hook.Run("PostDrawHUD")
		-- the vgui layer paints above the HUD (derma.luau), then the overlay
		if __derma_paint then __derma_paint() end
		hook.Run("DrawOverlay")
	end)
	-- key edges: the vgui layer takes the mouse / keyboard first, then `PlayerBindPress`-style hooks
	events.on("input.key", function(p)
		local code = __glua_key_edge(p.key, p.down)
		if __derma_key and __derma_key(p.key, p.down) then return end
		if code then
			-- the game's key hooks: PlayerButtonDown / Up for every key, PlayerBindPress for a
			-- bound command (returning true eats the bind)
			local lp = LocalPlayer()
			if p.down then
				hook.Run("PlayerButtonDown", lp, code)
				local bind = input.LookupKeyBinding(code)
				if bind then hook.Run("PlayerBindPress", lp, bind, true, code) end
			else
				hook.Run("PlayerButtonUp", lp, code)
			end
		end
	end)
end
