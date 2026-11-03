-- GLua client drawing surface over the host's `draw.*` command list (own implementation from
-- tables this realm builds or reads as documented: @struct ToScreenData @struct ViewSetup @struct FontData @struct DynamicLight @struct TextData
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
	render = render or {}
	for _, n in ipairs({ "AddBeam", "BlurRenderTarget", "BrushMaterialOverride", "Capture", "CapturePixels", "Clear", "ClearBuffersObeyStencil", "ClearDepth", "ClearRenderTarget", "ClearStencil", "ClearStencilBufferRectangle", "ComputeDynamicLighting", "ComputeLighting", "ComputePixelDiameterOfSphere", "CopyRenderTargetToTexture", "CopyTexture", "CullMode", "DepthRange", "DrawBeam", "DrawBox", "DrawLine", "DrawQuad", "DrawQuadEasy", "DrawScreenQuad", "DrawScreenQuadEx", "DrawSphere", "DrawSprite", "DrawTextureToScreen", "DrawTextureToScreenRect", "DrawWireframeBox", "DrawWireframeSphere", "EnableClipping", "EndBeam", "Fog*", "GetAmbientLightColor", "GetBlend", "GetColorModulation", "GetDXLevel", "GetFogColor", "GetFogDistances", "GetFogMode", "GetLightColor", "GetRenderTarget", "GetScreenEffectTexture", "GetToneMappingScaleLinear", "GetViewSetup", "MaterialOverride", "MaterialOverrideByIndex", "MaxTextureHeight", "MaxTextureWidth", "ModelMaterialOverride", "OverrideAlphaWriteEnable", "OverrideBlend", "OverrideBlendFunc", "OverrideColorWriteEnable", "OverrideDepthEnable", "PerformFullScreenStencilOperation", "PopCustomClipPlane", "PopFilterMag", "PopFilterMin", "PopFlashlightMode", "PopRenderTarget", "PushCustomClipPlane", "PushFilterMag", "PushFilterMin", "PushFlashlightMode", "PushRenderTarget", "ReadPixel", "RedownloadAllLightmaps", "RenderFlashlights", "RenderHUD", "RenderView", "ResetModelLighting", "ResetToneMappingScale", "SetAmbientLight", "SetBlend", "SetColorMaterial", "SetColorMaterialIgnoreZ", "SetColorModulation", "SetFogZ", "SetGoalToneMappingScale", "SetLightingMode", "SetLightingOrigin", "SetLocalModelLights", "SetMaterial", "SetModelLighting", "SetRenderTarget", "SetRenderTargetEx", "SetScissorRect", "SetShadowColor", "SetShadowDirection", "SetShadowDistance", "SetShadowsDisabled", "SetStencilCompareFunction", "SetStencilEnable", "SetStencilFailOperation", "SetStencilPassOperation", "SetStencilReferenceValue", "SetStencilTestMask", "SetStencilWriteMask", "SetStencilZFailOperation", "SetToneMappingScaleLinear", "SetViewPort", "SetWriteDepthToDestAlpha", "Spin", "StartBeam", "SupportsHDR", "SupportsPixelShaders_1_4", "SupportsPixelShaders_2_0", "SupportsVertexShaders_2_0", "SuppressEngineLighting", "UpdateFullScreenDepthTexture", "UpdatePowerOfTwoTexture", "UpdateRefractTexture", "UpdateScreenEffectTexture" }) do if n ~= "Fog*" then render[n] = stub("render." .. n) end end
	for _, n in ipairs({ "EyePos", "EyeAngles", "EyeVector", "ClientsideModel", "ClientsideRagdoll", "ClientsideScene", "CreateSprite", "ParticleEmitter", "CreateParticleSystem", "CreateParticleSystemNoEntity", "DynamicLight", "ProjectedTexture", "CreateMaterial", "DynamicMaterial", "AddWorldTip", "CreateContextMenu", "DisableClipping", "DrawBloom", "DrawBokehDOF", "DrawColorModify", "DrawMaterialOverlay", "DrawMotionBlur", "DrawSharpen", "DrawSobel", "DrawSunbeams", "DrawTexturize", "DrawToyTown", "DrawBackground", "DOF_Start", "DOF_Kill", "DOFModeHack" }) do _G[n] = stub("Global." .. n) end
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
function MAT:GetTexture(k) if k == "$basetexture" then return self.__dynamic end return nil end
function MAT:SetDynamicImage(tex) self.__dynamic = tex end
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
-- the host reads angles as x/y/z tables (pitch, yaw, roll)
function __glua_ang_xyz(a) return { x = a.p or a.x or 0, y = a.y or 0, z = a.r or a.z or 0 } end
function cam.Start3D2D(pos, ang, scale) hostdraw.start3d2d(pos, __glua_ang_xyz(ang), scale or 1) end
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
	__glua_real_dt = p.dt or 0
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


------------------------------------------------------------------------ spawnmenu registry
-- the sandbox spawn menu's registries (`spawnmenu.lua`, `creationmenu`, `toolmenu`): tool
-- tabs / categories / options, creation tabs, content types, prop categories; the native
-- menu in gm-client reads the game's text files itself, these hold what scripts add
spawnmenu = spawnmenu or {}
local tool_tabs, tool_tab_order = {}, {}
local creation_tabs, creation_order = {}, {}
local content_types, prop_tables, custom_props = {}, {}, {}
local active_cpanel, active_tool_tab, active_creation_tab
function spawnmenu.AddToolTab(name, label, icon)
	if not tool_tabs[name] then
		tool_tabs[name] = { Label = label or name, Icon = icon or "icon16/wrench.png", Items = {}, ItemOrder = {} }
		tool_tab_order[#tool_tab_order + 1] = name
	end
	return tool_tabs[name]
end
function spawnmenu.AddToolCategory(tab, name, label)
	local t = spawnmenu.AddToolTab(tab)
	if not t.Items[name] then
		t.Items[name] = { ItemName = name, Text = label or name, Items = {} }
		t.ItemOrder[#t.ItemOrder + 1] = name
	end
	return t.Items[name]
end
function spawnmenu.AddToolMenuOption(tab, category, class, name, command, config, cpanel, tbl)
	local cat = spawnmenu.AddToolCategory(tab, category)
	local item = tbl or {}
	item.ItemName, item.Text, item.Command, item.Config, item.CPanelFunction = class, name, command, config, cpanel
	for i, it in ipairs(cat.Items) do if it.ItemName == class then cat.Items[i] = item return item end end
	cat.Items[#cat.Items + 1] = item
	return item
end
function spawnmenu.GetToolMenu(name, label, icon)
	local t = spawnmenu.AddToolTab(name, label, icon)
	local out = {}
	for _, cname in ipairs(t.ItemOrder) do
		local c = t.Items[cname]
		local list = { ItemName = c.ItemName, Text = c.Text }
		for i, it in ipairs(c.Items) do list[i] = it end
		out[#out + 1] = list
	end
	return out
end
function spawnmenu.GetTools()
	local out = {}
	for _, name in ipairs(tool_tab_order) do
		local t = tool_tabs[name]
		out[#out + 1] = { Name = name, Label = t.Label, Icon = t.Icon, Items = spawnmenu.GetToolMenu(name) }
	end
	return out
end
function spawnmenu.ClearToolMenus() tool_tabs, tool_tab_order = {}, {} end
function spawnmenu.SwitchToolTab(id) active_tool_tab = id end
function spawnmenu.ActivateTool(name, menu_only)
	if not menu_only then RunConsoleCommand("gmod_tool", name) end
	for _, tab in pairs(tool_tabs) do
		for _, cat in pairs(tab.Items) do
			for _, it in ipairs(cat.Items) do
				if it.ItemName == name then spawnmenu.ActivateToolPanel(1, it) return end
			end
		end
	end
end
function spawnmenu.ActivateToolPanel(tab_id, item)
	local cp = vgui.Create("ControlPanel")
	if item.CPanelFunction then item.CPanelFunction(cp) end
	active_cpanel = cp
	hook.Run("SpawnMenuActivateToolPanel", tab_id, item)
	return cp
end
function spawnmenu.ActiveControlPanel() return active_cpanel end
function spawnmenu.SetActiveControlPanel(p) active_cpanel = p end
function spawnmenu.AddCreationTab(name, fn, material, order, tooltip)
	creation_tabs[name] = { Function = fn, Icon = material or "icon16/exclamation.png", Order = order or 1000, Tooltip = tooltip }
	creation_order[#creation_order + 1] = name
	table.sort(creation_order, function(a, b) local oa, ob = creation_tabs[a].Order, creation_tabs[b].Order; if oa ~= ob then return oa < ob end return a < b end)
end
function spawnmenu.GetCreationTabs() return creation_tabs end
function spawnmenu.SwitchCreationTab(name) active_creation_tab = name end
function spawnmenu.AddContentType(name, fn) content_types[name] = fn end
function spawnmenu.GetContentType(name, container, data)
	local fn = content_types[name]
	if fn and container then return fn(container, data) end
	return fn
end
function spawnmenu.CreateContentIcon(kind, container, data)
	local fn = content_types[kind]
	if fn then return fn(container, data) end
end
function spawnmenu.AddPropCategory(file, name, contents, icon, id, parentid, needsapp)
	prop_tables[file] = { name = name, contents = contents or {}, icon = icon or "icon16/page.png", id = id or 0, parentid = parentid or 0, needsapp = needsapp or "" }
	hook.Run("PopulatePropMenu")
end
function spawnmenu.GetPropTable() return prop_tables end
function spawnmenu.GetCustomPropTable() return custom_props end
function spawnmenu.PopulateFromTextFiles(fn)
	-- the engine's spawnlists are the client's own (settings/spawnlist/*.txt); the native menu
	-- mounts them, scripts see only what AddPropCategory added
	for file, t in pairs(prop_tables) do fn(t.name, t.icon, t.id, t.parentid, t.needsapp, t.contents) end
end
function spawnmenu.PopulateFromEngineTextFiles() hook.Run("PopulatePropMenu") end
function spawnmenu.SaveToTextFiles(tbl) custom_props = tbl or custom_props; hook.Run("SpawnlistContentChanged") end
function spawnmenu.DoSaveToTextFiles(tbl) spawnmenu.SaveToTextFiles(tbl) end
-- the game's own tabs so scripts can add next to them (sandbox `spawnmenu.AddCreationTab`)
spawnmenu.AddCreationTab("#spawnmenu.content_tab", function() return vgui.Create("SpawnmenuContentPanel") end, "icon16/application_view_tile.png", 10)
spawnmenu.AddToolTab("Main", "#spawnmenu.tools_tab", "icon16/wrench.png")
spawnmenu.AddToolTab("Utilities", "#spawnmenu.utilities", "icon16/page_white_wrench.png")
spawnmenu.AddToolTab("Options", "#spawnmenu.options", "icon16/wrench_orange.png")

------------------------------------------------------------------------ achievements
-- the game's achievements (Steam stats): the counters live locally here and the names / goals
-- are the documented sandbox set; `IsAchieved` when the count reaches the goal
achievements = achievements or {}
local ACH = {
	{ "Remover", "#achievements.remover", "#achievements.remover_desc", 1000 },
	{ "SpawnedProp", "#achievements.spawnedprop", "#achievements.spawnedprop_desc", 5000 },
	{ "SpawnedRagdoll", "#achievements.spawnedragdoll", "#achievements.spawnedragdoll_desc", 2000 },
	{ "SpawnedNPC", "#achievements.spawnednpc", "#achievements.spawnednpc_desc", 1000 },
	{ "SpawnMenuOpen", "#achievements.spawnmenuopen", "#achievements.spawnmenuopen_desc", 100000 },
	{ "EatBall", "#achievements.eatball", "#achievements.eatball_desc", 200 },
	{ "BalloonPopped", "#achievements.balloonpopped", "#achievements.balloonpopped_desc", 1000 },
	{ "IncBaddies", "#achievements.baddies", "#achievements.baddies_desc", 1000 },
	{ "IncGoodies", "#achievements.goodies", "#achievements.goodies_desc", 1000 },
	{ "IncBystander", "#achievements.bystander", "#achievements.bystander_desc", 1000 },
}
local ach_counts = {}
function achievements.Count() return #ACH end
function achievements.GetName(i) local a = ACH[(i or 0) + 1]; return a and a[2] or "" end
function achievements.GetDesc(i) local a = ACH[(i or 0) + 1]; return a and a[3] or "" end
function achievements.GetGoal(i) local a = ACH[(i or 0) + 1]; return a and a[4] or 0 end
function achievements.GetCount(i) local a = ACH[(i or 0) + 1]; return a and (ach_counts[a[1]] or 0) or 0 end
function achievements.IsAchieved(i) local a = ACH[(i or 0) + 1]; return a ~= nil and (ach_counts[a[1]] or 0) >= a[4] end
for _, a in ipairs(ACH) do
	achievements[a[1]] = function() ach_counts[a[1]] = (ach_counts[a[1]] or 0) + 1 end
end


------------------------------------------------------------------------ render
-- world geometry goes to the client as triangles (`draw.world_tri`) drawn with the 3D2D
-- planes; the colour material / SetMaterial state is kept (textures on world quads are not
-- drawn yet - they fill with the colour). Render targets, stencil and fog are recorded, not
-- applied (documented as open).
render = render or {}
local R = render
local render_mat, color_mod, blend, scissor = nil, { 1, 1, 1 }, 1, nil
local function wcol(c)
	c = c or Color(255, 255, 255, 255)
	return { r = (c.r or 255) * color_mod[1], g = (c.g or 255) * color_mod[2], b = (c.b or 255) * color_mod[3], a = (c.a or 255) * blend }
end
local function tri(a, b, c, color) hostdraw.world_tri(a, b, c, wcol(color)) end
function R.SetColorMaterial() render_mat = nil end
function R.SetColorMaterialIgnoreZ() render_mat = nil end
function R.SetMaterial(m) render_mat = m end
function R.SetColorModulation(r, g, b) color_mod = { r or 1, g or 1, b or 1 } end
function R.GetColorModulation() return color_mod[1], color_mod[2], color_mod[3] end
function R.SetBlend(a) blend = a or 1 end
function R.GetBlend() return blend end
function R.DrawQuad(a, b, c, d, color) tri(a, b, c, color); tri(a, c, d, color) end
function R.DrawQuadEasy(pos, normal, w, h, color, rot)
	local n = Vector(normal.x, normal.y, normal.z); n:Normalize()
	local up = math.abs(n.z) < 0.99 and Vector(0, 0, 1) or Vector(1, 0, 0)
	local right = n:Cross(up); right:Normalize()
	local u = right:Cross(n); u:Normalize()
	if rot and rot ~= 0 then
		local cs, sn = math.cos(math.rad(rot)), math.sin(math.rad(rot))
		right, u = right * cs + u * sn, u * cs - right * sn
	end
	local hw, hh = w / 2, h / 2
	R.DrawQuad(pos - right * hw + u * hh, pos + right * hw + u * hh, pos + right * hw - u * hh, pos - right * hw - u * hh, color)
end
function R.DrawLine(a, b, color, ignorez)
	-- a one-unit-thick ribbon facing the viewer's up: two triangles
	local d = b - a
	local len = d:Length()
	if len <= 0 then return end
	local side = d:Cross(Vector(0, 0, 1))
	if side:Length() < 1e-4 then side = d:Cross(Vector(1, 0, 0)) end
	side:Normalize()
	side = side * 0.5
	tri(a - side, b - side, b + side, color); tri(a - side, b + side, a + side, color)
end
local function box_corners(pos, ang, mins, maxs)
	local out = {}
	for i = 0, 7 do
		local v = Vector(bit.band(i, 1) == 0 and mins.x or maxs.x, bit.band(i, 2) == 0 and mins.y or maxs.y, bit.band(i, 4) == 0 and mins.z or maxs.z)
		if ang then v:Rotate(ang) end
		out[i] = pos + v
	end
	return out
end
local BOX_FACES = { { 0, 2, 3, 1 }, { 4, 5, 7, 6 }, { 0, 1, 5, 4 }, { 2, 6, 7, 3 }, { 0, 4, 6, 2 }, { 1, 3, 7, 5 } }
local BOX_EDGES = { { 0, 1 }, { 1, 3 }, { 3, 2 }, { 2, 0 }, { 4, 5 }, { 5, 7 }, { 7, 6 }, { 6, 4 }, { 0, 4 }, { 1, 5 }, { 2, 6 }, { 3, 7 } }
function R.DrawBox(pos, ang, mins, maxs, color)
	local c = box_corners(pos, ang, mins, maxs)
	for _, f in ipairs(BOX_FACES) do R.DrawQuad(c[f[1]], c[f[2]], c[f[3]], c[f[4]], color) end
end
function R.DrawWireframeBox(pos, ang, mins, maxs, color, ignorez)
	local c = box_corners(pos, ang, mins, maxs)
	for _, e in ipairs(BOX_EDGES) do R.DrawLine(c[e[1]], c[e[2]], color, ignorez) end
end
local function sphere_point(pos, r, lon, lat)
	local sl, cl = math.sin(lat), math.cos(lat)
	return pos + Vector(cl * math.cos(lon), cl * math.sin(lon), sl) * r
end
function R.DrawSphere(pos, r, longs, lats, color)
	longs, lats = math.max(3, longs or 8), math.max(2, lats or 8)
	for i = 0, lats - 1 do
		local la0, la1 = -math.pi / 2 + math.pi * i / lats, -math.pi / 2 + math.pi * (i + 1) / lats
		for j = 0, longs - 1 do
			local lo0, lo1 = 2 * math.pi * j / longs, 2 * math.pi * (j + 1) / longs
			R.DrawQuad(sphere_point(pos, r, lo0, la0), sphere_point(pos, r, lo1, la0), sphere_point(pos, r, lo1, la1), sphere_point(pos, r, lo0, la1), color)
		end
	end
end
function R.DrawWireframeSphere(pos, r, longs, lats, color, ignorez)
	longs, lats = math.max(3, longs or 8), math.max(2, lats or 8)
	for i = 1, lats - 1 do
		local la = -math.pi / 2 + math.pi * i / lats
		for j = 0, longs - 1 do R.DrawLine(sphere_point(pos, r, 2 * math.pi * j / longs, la), sphere_point(pos, r, 2 * math.pi * (j + 1) / longs, la), color, ignorez) end
	end
	for j = 0, longs - 1 do
		local lo = 2 * math.pi * j / longs
		for i = 0, lats - 1 do R.DrawLine(sphere_point(pos, r, lo, -math.pi / 2 + math.pi * i / lats), sphere_point(pos, r, lo, -math.pi / 2 + math.pi * (i + 1) / lats), color, ignorez) end
	end
end
local beam
function R.StartBeam(n) beam = {} end
function R.AddBeam(pos, width, tex, color) if beam then beam[#beam + 1] = { pos, width, color } end end
function R.EndBeam()
	if not beam then return end
	for i = 1, #beam - 1 do R.DrawBeam(beam[i][1], beam[i + 1][1], beam[i][2], 0, 1, beam[i][3]) end
	beam = nil
end
function R.DrawBeam(a, b, width, tex_start, tex_end, color)
	local d = b - a
	local side = d:Cross(Vector(0, 0, 1))
	if side:Length() < 1e-4 then side = d:Cross(Vector(1, 0, 0)) end
	side:Normalize()
	side = side * (width / 2)
	R.DrawQuad(a - side, b - side, b + side, a + side, color)
end
function R.DrawSprite(pos, w, h, color) R.DrawQuadEasy(pos, Vector(0, 0, 1), w, h, color, 0) end
function R.DrawScreenQuad() surface.SetDrawColor(255 * color_mod[1], 255 * color_mod[2], 255 * color_mod[3], 255 * blend); if render_mat then surface.SetMaterial(render_mat); surface.DrawTexturedRect(0, 0, ScrW(), ScrH()) else surface.DrawRect(0, 0, ScrW(), ScrH()) end end
function R.DrawScreenQuadEx(x, y, w, h) surface.SetDrawColor(255 * color_mod[1], 255 * color_mod[2], 255 * color_mod[3], 255 * blend); if render_mat then surface.SetMaterial(render_mat); surface.DrawTexturedRect(x, y, w, h) else surface.DrawRect(x, y, w, h) end end
function R.DrawTextureToScreen(tex) R.DrawScreenQuad() end
function R.DrawTextureToScreenRect(tex, x, y, w, h) R.DrawScreenQuadEx(x, y, w, h) end
function R.Clear(r, g, b, a, depth, stencil) surface.SetDrawColor(r or 0, g or 0, b or 0, a or 255); surface.DrawRect(0, 0, ScrW(), ScrH()) end
function R.ClearDepth() end
function R.ClearStencil() end
function R.ClearBuffersObeyStencil(r, g, b, a) R.Clear(r, g, b, a) end
function R.ClearStencilBufferRectangle() end
function R.ClearRenderTarget(tex, color) end
function R.SetScissorRect(x, y, x2, y2, on) scissor = on and { x, y, x2, y2 } or nil end
function R.GetDXLevel() return 95 end
function R.SupportsHDR() return true end
function R.SupportsPixelShaders_1_4() return true end
function R.SupportsPixelShaders_2_0() return true end
function R.SupportsVertexShaders_2_0() return true end
function R.MaxTextureWidth() return 4096 end
function R.MaxTextureHeight() return 4096 end
function R.GetAmbientLightColor() return Vector(0.2, 0.2, 0.2) end
function R.GetLightColor(pos) return Vector(1, 1, 1) end
function R.ComputeLighting(pos, normal) return Vector(1, 1, 1) end
function R.ComputeDynamicLighting(pos, normal) return Vector(0, 0, 0) end
function R.ComputePixelDiameterOfSphere(pos, r)
	local lp = LocalPlayer()
	local eye = lp and lp.EyePos and lp:EyePos() or Vector()
	local d = (pos - eye):Length()
	if d <= r then return ScrH() end
	local fov = lp and lp.GetFOV and lp:GetFOV() or 90
	return 2 * r / d / math.tan(math.rad(fov) / 2) * ScrH() / 2
end
local fog = { mode = 0, start = 0, ["end"] = 0, color = { 0, 0, 0 }, z = 0 }
function R.FogMode(m) fog.mode = m end
function R.FogStart(s) fog.start = s end
function R.FogEnd(e) fog["end"] = e end
function R.FogColor(r, g, b) fog.color = { r, g, b } end
function R.FogMaxDensity(d) fog.density = d end
function R.SetFogZ(z) fog.z = z end
function R.GetFogMode() return fog.mode end
function R.GetFogDistances() return fog.start, fog["end"], fog.z end
function R.GetFogColor() return fog.color[1], fog.color[2], fog.color[3] end
local rts = {}
function R.PushRenderTarget(tex) rts[#rts + 1] = tex end
function R.PopRenderTarget() rts[#rts] = nil end
function R.SetRenderTarget(tex) rts[#rts] = tex end
function R.SetRenderTargetEx(i, tex) rts[#rts] = tex end
function R.GetRenderTarget() return rts[#rts] end
function R.GetScreenEffectTexture(i) return Material("_rt_FullFrameFB" .. (i or 0)) end
function R.CopyRenderTargetToTexture() end
function R.CopyTexture() end
function R.UpdateScreenEffectTexture() end
function R.UpdateRefractTexture() end
function R.UpdateFullScreenDepthTexture() end
function R.UpdatePowerOfTwoTexture() return Material("_rt_PowerOfTwoFB") end
function R.BlurRenderTarget() end
function R.Capture(data) return "" end
function R.CapturePixels() end
function R.ReadPixel(x, y) return 0, 0, 0, 255 end
function R.SetViewPort(x, y, w, h) end
function R.GetViewSetup(main)
	local lp = LocalPlayer()
	return { origin = lp and lp.EyePos and lp:EyePos() or Vector(), angles = lp and lp.EyeAngles and lp:EyeAngles() or Angle(), fov = lp and lp.GetFOV and lp:GetFOV() or 90, x = 0, y = 0, width = ScrW(), height = ScrH(), znear = 3, zfar = 28377.9, aspect = ScrW() / ScrH() }
end
function R.RenderView(view) hook.Run("RenderScene", view and view.origin, view and view.angles, view and view.fov) end
function R.RenderHUD(x, y, w, h) hook.Run("HUDPaint") end
function R.RenderFlashlights() end
function R.Spin() end
function R.SetLightingMode() end
function R.SetLightingOrigin() end
function R.SetLocalModelLights() end
function R.SetModelLighting() end
function R.ResetModelLighting() end
function R.SetAmbientLight() end
function R.SuppressEngineLighting() end
function R.SetShadowColor() end
function R.SetShadowDirection() end
function R.SetShadowDistance() end
function R.SetShadowsDisabled() end
function R.SetToneMappingScaleLinear() end
function R.GetToneMappingScaleLinear() return Vector(1, 1, 1) end
function R.SetGoalToneMappingScale() end
function R.ResetToneMappingScale() end
function R.PushFilterMag() end
function R.PopFilterMag() end
function R.PushFilterMin() end
function R.PopFilterMin() end
function R.PushFlashlightMode() end
function R.PopFlashlightMode() end
function R.PushCustomClipPlane() end
function R.PopCustomClipPlane() end
function R.EnableClipping(b) return false end
function R.CullMode() end
function R.DepthRange() end
function R.OverrideAlphaWriteEnable() end
function R.OverrideBlend() end
function R.OverrideBlendFunc() end
function R.OverrideColorWriteEnable() end
function R.OverrideDepthEnable() end
function R.SetWriteDepthToDestAlpha() end
function R.MaterialOverride(m) end
function R.MaterialOverrideByIndex() end
function R.ModelMaterialOverride(m) end
function R.BrushMaterialOverride(m) end
function R.SetStencilEnable() end
function R.SetStencilCompareFunction() end
function R.SetStencilFailOperation() end
function R.SetStencilPassOperation() end
function R.SetStencilZFailOperation() end
function R.SetStencilReferenceValue() end
function R.SetStencilTestMask() end
function R.SetStencilWriteMask() end
function R.PerformFullScreenStencilOperation() end
function R.RedownloadAllLightmaps() end
function cam.Start3D() end


------------------------------------------------------------------------ view, client entities, particles, lights
-- the local view: the local player's eye (the client's own camera is not scripted yet)
function EyePos() local lp = LocalPlayer(); return IsValid(lp) and lp:EyePos() or Vector() end
function EyeAngles() local lp = LocalPlayer(); return IsValid(lp) and lp:EyeAngles() or Angle() end
function EyeVector() return EyeAngles():Forward() end
local VEC = __glua_classes.Vector
-- `Vector:ToScreen()`: the point through the view's perspective (fov across the width as the
-- engine does: tan(fov/2) scaled by the aspect), `visible` when in front of the view
function VEC:ToScreen()
	local eye, ang = EyePos(), EyeAngles()
	local lp = LocalPlayer()
	local fov = IsValid(lp) and lp:GetFOV() or 90
	local d = self - eye
	local fwd, right, up = ang:Forward(), ang:Right(), ang:Up()
	local z = d:Dot(fwd)
	local w, h = ScrW(), ScrH()
	local tan_half = math.tan(math.rad(fov) / 2)
	if z <= 0.001 then return { x = -1, y = -1, visible = false } end
	local x = d:Dot(right) / (z * tan_half * (w / h))
	local y = d:Dot(up) / (z * tan_half)
	return { x = (x + 1) / 2 * w, y = (1 - y) / 2 * h, visible = x >= -1 and x <= 1 and y >= -1 and y <= 1 }
end

-- clientside entities: `ClientsideModel` / `ClientsideRagdoll` / `CreateSprite` return a CSEnt
-- drawn by the client each frame through `draw.model` while it has a model and is not NoDraw
local CSENT = {}
CSENT.__index = CSENT
local csents = {}
__glua_classes.CSEnt = CSENT
local function new_csent(model, group)
	local e = setmetatable({ __model = model or "", __pos = Vector(), __ang = Angle(), __valid = true, __nodraw = false, __color = Color(255, 255, 255, 255), __group = group, __skin = 0, __bodygroups = {}, __parent = nil, __scale = 1, __material = "", __sequence = 0, __cycle = 0, __rate = 1, __rendermode = 0, __fx = 0 }, CSENT)
	csents[e] = true
	return e
end
function ClientsideModel(model, group) return new_csent(model, group or 0) end
function ClientsideRagdoll(model, group) local e = new_csent(model, group or 0); e.__ragdoll = true; return e end
function ClientsideScene(name, target) return new_csent("", 0) end
function CreateSprite(mat) local e = new_csent("", 0); e.__sprite = mat; return e end
function CSENT:IsValid() return self.__valid == true end
function CSENT:Remove() self.__valid = false; csents[self] = nil end
function CSENT:SetModel(m) self.__model = m end
function CSENT:GetModel() return self.__model end
function CSENT:SetPos(p) self.__pos = p end
function CSENT:GetPos() return self.__pos end
function CSENT:SetAngles(a) self.__ang = a end
function CSENT:GetAngles() return self.__ang end
function CSENT:SetNoDraw(b) self.__nodraw = b == true end
function CSENT:GetNoDraw() return self.__nodraw end
function CSENT:SetColor(c) self.__color = c end
function CSENT:GetColor() return self.__color end
function CSENT:SetRenderMode(m) self.__rendermode = m end
function CSENT:GetRenderMode() return self.__rendermode end
function CSENT:SetRenderFX(f) self.__fx = f end
function CSENT:SetSkin(s) self.__skin = s end
function CSENT:GetSkin() return self.__skin end
function CSENT:SetBodygroup(i, v) self.__bodygroups[i] = v end
function CSENT:GetBodygroup(i) return self.__bodygroups[i] or 0 end
function CSENT:SetModelScale(s) self.__scale = s end
function CSENT:GetModelScale() return self.__scale end
function CSENT:SetMaterial(m) self.__material = m end
function CSENT:GetMaterial() return self.__material end
function CSENT:SetParent(p) self.__parent = p end
function CSENT:GetParent() return self.__parent end
function CSENT:SetSequence(s) self.__sequence = s end
function CSENT:GetSequence() return self.__sequence end
function CSENT:SetCycle(c) self.__cycle = c end
function CSENT:GetCycle() return self.__cycle end
function CSENT:SetPlaybackRate(r) self.__rate = r end
function CSENT:FrameAdvance(dt) self.__cycle = (self.__cycle + (dt or FrameTime()) * self.__rate) % 1 end
function CSENT:LookupSequence(name) return -1, 0 end
function CSENT:LookupBone(name) return nil end
function CSENT:LookupAttachment(name) return 0 end
function CSENT:GetBonePosition(i) return self.__pos, self.__ang end
function CSENT:SetupBones() end
function CSENT:Spawn() end
function CSENT:Activate() end
function CSENT:SetRenderOrigin(p) self.__render_origin = p end
function CSENT:SetRenderAngles(a) self.__render_angles = a end
function CSENT:SetRenderBounds(mins, maxs) self.__bounds = { mins, maxs } end
function CSENT:SetLOD() end
function CSENT:SetPoseParameter() end
function CSENT:SetEyeTarget() end
function CSENT:SetFlexWeight() end
function CSENT:GetFlexNum() return 0 end
function CSENT:IsDormant() return false end
function CSENT:EntIndex() return -1 end
function CSENT:GetClass() return self.__ragdoll and "class C_ClientRagdoll" or "class C_BaseFlex" end
function CSENT:GetTable() return self end
function CSENT:OBBMins() return Vector(-16, -16, 0) end
function CSENT:OBBMaxs() return Vector(16, 16, 72) end
function CSENT:WorldSpaceCenter() return self.__pos end
function CSENT:DrawModel(flags)
	if self.__model == "" then return end
	hostdraw.model(self.__model, self.__render_origin or self.__pos, __glua_ang_xyz(self.__render_angles or self.__ang), self.__color.a or 255)
end
function CSENT:DrawShadow() end
function CSENT:CreateShadow() end
function CSENT:DestroyShadow() end
local function draw_csents()
	for e in pairs(csents) do
		if e.__valid and not e.__nodraw then
			if e.__sprite then
				render.SetMaterial(type(e.__sprite) == "string" and Material(e.__sprite) or e.__sprite)
				render.DrawSprite(e.__pos, 16 * e.__scale, 16 * e.__scale, e.__color)
			else
				e:DrawModel()
			end
		end
	end
end

-- client entity extras over the networked entities (render overrides are kept per entity)
local ENT = __glua_classes.Entity
local render_over = setmetatable({}, { __mode = "k" })
local function ro(self) local r = render_over[self]; if not r then r = {}; render_over[self] = r end return r end
function ENT:SetRenderOrigin(p) ro(self).origin = p end
function ENT:GetRenderOrigin() return ro(self).origin end
function ENT:SetRenderAngles(a) ro(self).angles = a end
function ENT:GetRenderAngles() return ro(self).angles end
function ENT:SetRenderBounds(mins, maxs, add) ro(self).bounds = { mins, maxs } end
function ENT:SetRenderBoundsWS(mins, maxs, add) ro(self).bounds_ws = { mins, maxs } end
function ENT:GetRenderBounds() local b = ro(self).bounds; if b then return b[1], b[2] end return self:OBBMins(), self:OBBMaxs() end
function ENT:SetRenderClipPlane(n, d) ro(self).clip = { n, d } end
function ENT:SetRenderClipPlaneEnabled(b) ro(self).clip_on = b == true end
function ENT:GetRenderGroup() return ro(self).group or 7 end
function ENT:DrawModel(flags) hostdraw.model(self:GetModel(), ro(self).origin or self:GetPos(), __glua_ang_xyz(ro(self).angles or self:GetAngles()), 255) end
function ENT:GetNetworkAngles() return self:GetAngles() end
function ENT:SetNetworkAngles(a) end
function ENT:GetPredictable() return false end
function ENT:SetPredictable(b) end
function ENT:SetupBones() end
function ENT:InvalidateBoneCache() end
function ENT:SetBonePosition(i, pos, ang) end
function ENT:SetLOD(l) end
function ENT:SetLegacyTransform(b) end
function ENT:SetNextClientThink(t) ro(self).next_think = t end
function ENT:GetAnimTime() return ro(self).anim_time or 0 end
function ENT:SetAnimTime(t) ro(self).anim_time = t end
function ENT:GetModelPhysBoneCount() local m = self.GetModel and self:GetModel(); return (m and m:find("ragdoll") and 15) or 1 end
function ENT:BeingLookedAtByLocalPlayer()
	local lp = LocalPlayer()
	if not IsValid(lp) then return false end
	local tr = util.TraceLine({ start = lp:EyePos(), endpos = lp:EyePos() + lp:GetAimVector() * 1024, filter = lp })
	return tr.Entity == self
end
function ENT:BecomeRagdollOnClient() local e = ClientsideRagdoll(self:GetModel()); e:SetPos(self:GetPos()); e:SetAngles(self:GetAngles()); return e end
function ENT:InitializeAsClientEntity() end
function ENT:SnatchModelInstance(other) end
function ENT:EnableMatrix(name, m) ro(self).matrix = m end
function ENT:DisableMatrix(name) ro(self).matrix = nil end
function ENT:CreateShadow() end
function ENT:DestroyShadow() end
function ENT:MarkShadowAsDirty() end
function ENT:SetupPhonemeMappings() end
function ENT:UpdateShadow() end
function ENT:UpdateBoneFollowers() end
function ENT:UpdateRenderInfo() end
function ENT:CreateParticleEffect(name, attach, options) return CreateParticleSystem(self, name, attach or 0) end
function ENT:StopParticleEmission() end
function ENT:StopParticlesNamed(name) end
function ENT:StopParticlesWithNameAndAttachment(name, attach) end
function ENT:StopAndDestroyParticles() end
local PLY = __glua_classes.Player
local muted, voice_scale, flashlight_colors = {}, {}, {}
function PLY:IsMuted() return muted[self:SteamID64() or self:EntIndex()] == true end
function PLY:SetMuted(b) muted[self:SteamID64() or self:EntIndex()] = b == true end
function PLY:GetVoiceVolumeScale() return voice_scale[self:EntIndex()] or 1 end
function PLY:SetVoiceVolumeScale(s) voice_scale[self:EntIndex()] = s end
function PLY:VoiceVolume() return 0 end
function PLY:IsVoiceAudible() return false end
function PLY:GetFriendStatus() return "none" end
function PLY:ShowProfile() if self.SteamID64 then gui.OpenURL("https://steamcommunity.com/profiles/" .. tostring(self:SteamID64())) end end
function PLY:GetFlashlightColor() return flashlight_colors[self:EntIndex()] or Color(255, 255, 255) end
function PLY:SetFlashlightColor(c) flashlight_colors[self:EntIndex()] = c end
function PLY:ShouldDrawLocalPlayer() return hook.Run("ShouldDrawLocalPlayer", self) == true end
function PLY:AddPlayerOption(name, time, vote_fn, draw_fn) end

-- particles: `ParticleEmitter(pos, use3d)` simulates CLuaParticles here (gravity, air
-- resistance, roll, colour / alpha / size lerp over life) and draws them as camera-facing
-- quads in the world each frame
local EMITTER, PARTICLE = {}, {}
EMITTER.__index, PARTICLE.__index = EMITTER, PARTICLE
__glua_classes.CLuaEmitter, __glua_classes.CLuaParticle = EMITTER, PARTICLE
local emitters = {}
function ParticleEmitter(pos, use3d) local e = setmetatable({ __pos = pos or Vector(), __3d = use3d == true, __particles = {}, __valid = true, __nodraw = false, __near = 0 }, EMITTER); emitters[e] = true; return e end
function EMITTER:IsValid() return self.__valid end
function EMITTER:Finish() self.__finished = true end
function EMITTER:GetPos() return self.__pos end
function EMITTER:SetPos(p) self.__pos = p end
function EMITTER:Is3D() return self.__3d end
function EMITTER:GetNumActiveParticles() return #self.__particles end
function EMITTER:SetNoDraw(b) self.__nodraw = b == true end
function EMITTER:SetNearClip(a, b) self.__near = a end
function EMITTER:SetBBox(mins, maxs) self.__bbox = { mins, maxs } end
function EMITTER:SetParticleCullRadius(r) self.__cull = r end
function EMITTER:Draw() for _, p in ipairs(self.__particles) do p:__draw() end end
function EMITTER:Add(material, pos)
	local p = setmetatable({ __mat = material, pos = pos or self.__pos, vel = Vector(), ang = Angle(), angvel = Angle(), die = 1, life = 0, start_alpha = 255, end_alpha = 255, start_size = 1, end_size = 1, start_len = 0, end_len = 0, gravity = Vector(), air = 0, bounce = 0, roll = 0, roll_delta = 0, color = { 255, 255, 255 }, collide = false, lighting = false, vel_scale = true, next_think = 0 }, PARTICLE)
	self.__particles[#self.__particles + 1] = p
	return p
end
function PARTICLE:SetPos(p) self.pos = p end function PARTICLE:GetPos() return self.pos end
function PARTICLE:SetVelocity(v) self.vel = v end function PARTICLE:GetVelocity() return self.vel end
function PARTICLE:SetAngles(a) self.ang = a end function PARTICLE:GetAngles() return self.ang end
function PARTICLE:SetAngleVelocity(a) self.angvel = a end function PARTICLE:GetAngleVelocity() return self.angvel end
function PARTICLE:SetDieTime(t) self.die = t end function PARTICLE:GetDieTime() return self.die end
function PARTICLE:SetLifeTime(t) self.life = t end function PARTICLE:GetLifeTime() return self.life end
function PARTICLE:SetStartAlpha(a) self.start_alpha = a end function PARTICLE:GetStartAlpha() return self.start_alpha end
function PARTICLE:SetEndAlpha(a) self.end_alpha = a end function PARTICLE:GetEndAlpha() return self.end_alpha end
function PARTICLE:SetStartSize(s) self.start_size = s end function PARTICLE:GetStartSize() return self.start_size end
function PARTICLE:SetEndSize(s) self.end_size = s end function PARTICLE:GetEndSize() return self.end_size end
function PARTICLE:SetStartLength(l) self.start_len = l end function PARTICLE:GetStartLength() return self.start_len end
function PARTICLE:SetEndLength(l) self.end_len = l end function PARTICLE:GetEndLength() return self.end_len end
function PARTICLE:SetGravity(g) self.gravity = g end function PARTICLE:GetGravity() return self.gravity end
function PARTICLE:SetAirResistance(a) self.air = a end function PARTICLE:GetAirResistance() return self.air end
function PARTICLE:SetBounce(b) self.bounce = b end function PARTICLE:GetBounce() return self.bounce end
function PARTICLE:SetRoll(r) self.roll = r end function PARTICLE:GetRoll() return self.roll end
function PARTICLE:SetRollDelta(r) self.roll_delta = r end function PARTICLE:GetRollDelta() return self.roll_delta end
function PARTICLE:SetColor(r, g, b) self.color = { r, g, b } end function PARTICLE:GetColor() return self.color[1], self.color[2], self.color[3] end
function PARTICLE:SetMaterial(m) self.__mat = m end function PARTICLE:GetMaterial() return self.__mat end
function PARTICLE:SetCollide(b) self.collide = b == true end
function PARTICLE:SetCollideCallback(fn) self.on_collide = fn end
function PARTICLE:SetLighting(b) self.lighting = b == true end
function PARTICLE:SetVelocityScale(b) self.vel_scale = b ~= false end
function PARTICLE:SetThinkFunction(fn) self.think = fn end
function PARTICLE:SetNextThink(t) self.next_think = t end
function PARTICLE:__step(dt)
	self.life = self.life + dt
	self.vel = self.vel + self.gravity * dt
	if self.air > 0 then self.vel = self.vel * math.max(0, 1 - self.air * dt / 100) end
	self.pos = self.pos + self.vel * dt
	self.roll = self.roll + self.roll_delta * dt
	self.ang = self.ang + self.angvel * dt
	if self.think and CurTime() >= self.next_think then self:think() end
	return self.life < self.die
end
function PARTICLE:__draw()
	local f = self.die > 0 and math.min(1, self.life / self.die) or 1
	local a = self.start_alpha + (self.end_alpha - self.start_alpha) * f
	local size = self.start_size + (self.end_size - self.start_size) * f
	local c = Color(self.color[1], self.color[2], self.color[3], a)
	render.SetMaterial(type(self.__mat) == "string" and Material(self.__mat) or self.__mat)
	local len = self.start_len + (self.end_len - self.start_len) * f
	if len > 0 and self.vel:Length() > 0 then
		local d = self.vel:GetNormalized() * (len / 2)
		render.DrawBeam(self.pos - d, self.pos + d, size, 0, 1, c)
	else
		render.DrawQuadEasy(self.pos, -EyeVector(), size * 2, size * 2, c, self.roll)
	end
end
local function step_particles()
	local dt = FrameTime()
	for e in pairs(emitters) do
		if not e.__valid then emitters[e] = nil
		else
			local keep = {}
			for _, p in ipairs(e.__particles) do if p:__step(dt) then keep[#keep + 1] = p end end
			e.__particles = keep
			if not e.__nodraw then e:Draw() end
			if e.__finished and #keep == 0 then e.__valid = false; emitters[e] = nil end
		end
	end
end
-- particle systems (.pcf): the effect name and attachment are kept; the system itself needs
-- the client's particle renderer (open) - this object answers the documented interface
local CNP = {}
CNP.__index = CNP
__glua_classes.CNewParticleEffect = CNP
function CreateParticleSystem(ent, effect, attach, angles, offset) return setmetatable({ __ent = ent, __effect = effect, __attach = attach, __valid = true, __cp = {} }, CNP) end
function CreateParticleSystemNoEntity(effect, pos, ang) return setmetatable({ __effect = effect, __pos = pos, __ang = ang, __valid = true, __cp = {} }, CNP) end
function CNP:IsValid() return self.__valid end
function CNP:StopEmission() self.__stopped = true end
function CNP:StopEmissionAndDestroyImmediately() self.__valid = false end
function CNP:Restart() self.__stopped = false end
function CNP:IsFinished() return self.__stopped == true end
function CNP:IsViewModelEffect() return false end
function CNP:GetEffectName() return self.__effect end
function CNP:GetOwner() return self.__ent end
function CNP:SetControlPoint(i, v) self.__cp[i] = v end
function CNP:GetControlPoint(i) return self.__cp[i] end
function CNP:SetControlPointEntity(i, e) self.__cp[i] = e end
function CNP:SetControlPointForwardVector() end
function CNP:SetControlPointRightVector() end
function CNP:SetControlPointUpVector() end
function CNP:SetControlPointOrientation() end
function CNP:SetControlPointParent() end
function CNP:AddControlPoint(i, ent, attach) self.__cp[i] = ent end
function CNP:SetIsViewModelEffect() end
function CNP:SetShouldDraw(b) self.__draw = b end
function CNP:SetSortOrigin(p) end
function CNP:GetAutoUpdateBBox() return true end
function CNP:SetAutoUpdateBBox() end
function CNP:GetHighestControlPoint() return 0 end
function CNP:GetRenderBounds() return Vector(), Vector() end
function CNP:Render() end
function CNP:StartEmission() self.__stopped = false end

-- lights: DynamicLight / ProjectedTexture keep the documented state; the client's lighting
-- does not read them yet (open)
local DLIGHT = {}
function DynamicLight(index, elight) return { Pos = Vector(), r = 255, g = 255, b = 255, Brightness = 1, Size = 0, Decay = 0, DieTime = 0, Style = 0, Dir = Vector(), InnerAngle = 0, OuterAngle = 0, MinLight = 0, NoModel = false, NoWorld = false, Key = index } end
local PT = {}
PT.__index = PT
__glua_classes.ProjectedTexture = PT
local pt_fields = { Angles = Angle(), Pos = Vector(), Brightness = 1, Color = Color(255, 255, 255), ConstantAttenuation = 0, LinearAttenuation = 0, QuadraticAttenuation = 0, EnableShadows = false, FarZ = 1000, NearZ = 1, HorizontalFOV = 90, VerticalFOV = 90, LightWorld = true, NoCull = false, Orthographic = false, ShadowDepthBias = 0.0001, ShadowFilter = 0.5, ShadowSlopeScaleDepthBias = 2, SkipShadowUpdates = 0, TargetEntity = NULL, Texture = nil, TextureFrame = 0 }
function ProjectedTexture() local t = {}; for k, v in pairs(pt_fields) do t[k] = v end t.__valid = true; return setmetatable(t, PT) end
for k in pairs(pt_fields) do
	PT["Get" .. k] = function(self) return self[k] end
	PT["Set" .. k] = function(self, v) self[k] = v end
end
function PT:IsValid() return self.__valid end
function PT:Remove() self.__valid = false end
function PT:Update() end
function PT:SetFOV(f) self.HorizontalFOV, self.VerticalFOV = f, f end
function PT:GetOrthographic() return self.Orthographic end
function PT:SetOrthographic(b, l, t, r, bt) self.Orthographic = b == true; self.__ortho = { l, t, r, bt } end

-- materials made by scripts and the world tip
function CreateMaterial(name, shader, data) local m = Material(name, ""); m.__shader, m.__data = shader, data or {}; function m:GetString(k) return self.__data[k] end function m:SetString(k, v) self.__data[k] = v end return m end
function DynamicMaterial(shader, data) return CreateMaterial("dyn_" .. tostring(SysTime()), shader, data) end
local world_tip
function AddWorldTip(id, text, delay, pos, ent) world_tip = { text = text, pos = pos or (IsValid(ent) and ent:GetPos()), ent = ent, time = CurTime() } end
-- the game's world tip: black-bordered rounded box above the aimed point, DermaDefault text
local function draw_world_tip()
	if not world_tip or CurTime() - world_tip.time > 0.1 then return end
	local pos = world_tip.pos or (IsValid(world_tip.ent) and world_tip.ent:GetPos())
	if not pos then return end
	local sp = pos:ToScreen()
	if not sp.visible then return end
	surface.SetFont("GModWorldtip")
	local w, h = surface.GetTextSize(world_tip.text)
	local x, y = sp.x - w / 2 - 10, sp.y - h - 40
	draw.RoundedBox(8, x - 1, y - 1, w + 22, h + 22, Color(0, 0, 0, 255))
	draw.RoundedBox(8, x, y, w + 20, h + 20, Color(250, 250, 200, 255))
	draw.SimpleText(world_tip.text, "GModWorldtip", x + 10, y + 10, Color(0, 0, 0, 255))
end
function CreateContextMenu() hook.Run("OnContextMenuOpen") end
function DisableClipping(b) return false end
-- screen-space effects: recorded, not applied (the client's post stack is native; open)
for _, n in ipairs({ "DrawBloom", "DrawBokehDOF", "DrawColorModify", "DrawMaterialOverlay", "DrawMotionBlur", "DrawSharpen", "DrawSobel", "DrawSunbeams", "DrawTexturize", "DrawToyTown", "DrawBackground", "DOF_Start", "DOF_Kill", "DOFModeHack" }) do _G[n] = function() end end
function __glua_draw_world_objects() draw_csents(); step_particles(); draw_world_tip() end

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
		-- the scene hooks of the game's frame (world geometry from them goes to the 3D pass)
		hook.Run("PreRender")
		hook.Run("PreDrawOpaqueRenderables", false, false, false)
		hook.Run("PostDrawOpaqueRenderables", false, false, false)
		hook.Run("PreDrawTranslucentRenderables", false, false, false)
		hook.Run("PostDrawTranslucentRenderables", false, false, false)
		__glua_draw_world_objects()
		hook.Run("PreDrawEffects")
		hook.Run("PostDrawEffects")
		hook.Run("RenderScreenspaceEffects")
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
		hook.Run("PostRender")
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

-- ---------------------------------------------------------------------------------------------
-- remaining client globals: screen scale, frame time, view, model info, scheme colours,
-- language, render targets, Mesh, require/module, presets, spawn icons
-- ---------------------------------------------------------------------------------------------
__glua_real_dt = 0
function RealFrameTime() return __glua_real_dt end
function ScreenScale(n) return n * (ScrW() / 640) end
function ScreenScaleH(n) return n * (ScrH() / 480) end
SScale = ScreenScale
function GetViewEntity() local lp = LocalPlayer() return IsValid(lp) and lp:GetViewEntity() or lp end
function MainEyePos() return EyePos() end
function MainEyeAngles() return EyeAngles() end
function RenderAngles() return EyeAngles() end
function NumModelSkins(model)
	local ok, info = pcall(game.info, model)
	return ok and type(info) == "table" and tonumber(info.skins) or 0
end
function GetTimeoutInfo() return false, 0 end
function Error(...) local t = {} for i = 1, select("#", ...) do t[i] = tostring(select(i, ...)) end error(table.concat(t), 0) end

-- named colours of the client scheme (resource/ClientScheme.res)
local scheme_colors = {
	White = Color(255, 255, 255), OffWhite = Color(221, 221, 221), DullWhite = Color(190, 190, 190), Black = Color(0, 0, 0),
	TransparentBlack = Color(0, 0, 0, 128), Blank = Color(0, 0, 0, 0), Red = Color(255, 0, 0), Green = Color(0, 255, 0),
	Blue = Color(0, 0, 255), Yellow = Color(255, 255, 0), Gray = Color(136, 136, 136), DarkGray = Color(72, 72, 72),
	Orange = Color(255, 176, 0), Cyan = Color(0, 255, 255), Magenta = Color(255, 0, 255), LightGray = Color(200, 200, 200),
	HudIcon_Green = Color(0, 160, 0), HudIcon_Red = Color(255, 64, 64), HudStatusBg = Color(0, 0, 0, 100),
}
function NamedColor(name) local c = scheme_colors[name] return c and Color(c.r, c.g, c.b, c.a) or nil end

-- language: '#token' phrases, addons add their own
language = language or {}
local phrases = {}
function language.Add(token, translation) phrases[token] = translation end
function language.GetPhrase(token)
	local t = tostring(token or ""):gsub("^#", "")
	return phrases[t] or token
end
function Localize(token, default) local t = tostring(token or ""):gsub("^#", "") return phrases[t] or default or token end

-- render targets: ITexture objects by name (the client has no RT storage - open)
local render_targets = {}
local TEX = {}
TEX.__index = TEX
function TEX:GetName() return self.__name end
function TEX:Width() return self.__w end
function TEX:Height() return self.__h end
function TEX:GetMappingWidth() return self.__w end
function TEX:GetMappingHeight() return self.__h end
function TEX:IsError() return false end
function TEX:IsErrorTexture() return false end
function TEX:GetNumAnimationFrames() return 1 end
function TEX:GetColor(x, y) return Color(0, 0, 0, 255) end
function TEX:Download() end
function TEX:GetFlags() return self.__flags or 0 end
function TEX:GetImageFormat() return IMAGE_FORMAT_RGBA8888 end
function GetRenderTargetEx(name, w, h, size_mode, depth_mode, flags, rt_flags, format)
	local t = render_targets[name]
	if not t then
		t = setmetatable({ __name = name, __w = w, __h = h, __flags = flags or 0, __format = format }, TEX)
		render_targets[name] = t
	end
	return t
end
function GetRenderTarget(name, w, h) return GetRenderTargetEx(name, w, h, RT_SIZE_NO_CHANGE, MATERIAL_RT_DEPTH_SEPARATE, 0, 0, IMAGE_FORMAT_RGBA8888) end
function AddonMaterial(path) return Material(path) end

-- Mesh / mesh.*: vertices collected in Lua, drawn through the same world triangles render.* uses
local MESH = {}
MESH.__index = MESH
local building
local function mesh_triangles(prim, verts)
	local tris = {}
	local n = #verts
	if prim == MATERIAL_TRIANGLES or prim == nil then
		for i = 1, n - 2, 3 do tris[#tris + 1] = { verts[i], verts[i + 1], verts[i + 2] } end
	elseif prim == MATERIAL_TRIANGLE_STRIP then
		for i = 1, n - 2 do
			if i % 2 == 1 then tris[#tris + 1] = { verts[i], verts[i + 1], verts[i + 2] } else tris[#tris + 1] = { verts[i + 1], verts[i], verts[i + 2] } end
		end
	elseif prim == MATERIAL_QUADS then
		for i = 1, n - 3, 4 do
			tris[#tris + 1] = { verts[i], verts[i + 1], verts[i + 2] }
			tris[#tris + 1] = { verts[i], verts[i + 2], verts[i + 3] }
		end
	elseif prim == MATERIAL_POLYGON then
		for i = 2, n - 1 do tris[#tris + 1] = { verts[1], verts[i], verts[i + 1] } end
	elseif prim == MATERIAL_LINES or prim == MATERIAL_LINE_STRIP or prim == MATERIAL_LINE_LOOP or prim == MATERIAL_POINTS then
		-- lines and points have no area: nothing to draw as triangles
	end
	return tris
end
local function draw_tris(tris)
	for _, t in ipairs(tris) do
		local c = t[1].color or { r = 255, g = 255, b = 255, a = 255 }
		hostdraw.world_tri(t[1].pos, t[2].pos, t[3].pos, { c.r / 255, c.g / 255, c.b / 255, (c.a or 255) / 255 })
	end
end
function Mesh(mat) return setmetatable({ __valid = true, __material = mat, __tris = {} }, MESH) end
function MESH:IsValid() return self.__valid == true end
function MESH:Destroy() self.__valid = false; self.__tris = {} end
function MESH:Draw() if self.__valid then draw_tris(self.__tris) end end
function MESH:DrawSkinned(bones, count) self:Draw() end
__glua_classes.IMesh = MESH
function MESH:BuildFromTriangles(verts)
	local vs = {}
	for i, v in ipairs(verts) do vs[i] = { pos = v.pos, normal = v.normal, u = v.u, v = v.v, color = v.color } end
	self.__tris = mesh_triangles(MATERIAL_TRIANGLES, vs)
end
mesh = mesh or {}
function mesh.Begin(target, prim, count)
	if type(target) == "table" and getmetatable(target) == MESH then
		building = { mesh = target, prim = prim, verts = {}, cur = {} }
	else
		building = { mesh = nil, prim = target, verts = {}, cur = {} }
	end
end
function mesh.Position(p) if building then building.cur.pos = p end end
function mesh.Normal(n) if building then building.cur.normal = n end end
function mesh.Color(r, g, b, a) if building then building.cur.color = Color(r, g, b, a) end end
function mesh.TexCoord(stage, u, v) if building then building.cur.u, building.cur.v = u, v end end
function mesh.TangentS(t) if building then building.cur.tangent_s = t end end
function mesh.TangentT(t) if building then building.cur.tangent_t = t end end
function mesh.Specular(r, g, b, a) if building then building.cur.specular = Color(r, g, b, a) end end
function mesh.UserData(x, y, z, w) if building then building.cur.userdata = { x, y, z, w } end end
function mesh.AdvanceVertex()
	if not building then return end
	building.verts[#building.verts + 1] = building.cur
	building.cur = {}
end
function mesh.VertexCount() return building and #building.verts or 0 end
function mesh.End()
	if not building then return end
	local tris = mesh_triangles(building.prim, building.verts)
	if building.mesh then building.mesh.__tris = tris else draw_tris(tris) end
	building = nil
end
function mesh.Quad(a, b, c, d) if building then for _, p in ipairs({ a, b, c, d }) do building.verts[#building.verts + 1] = { pos = p, color = building.cur.color } end end end
function mesh.QuadEasy(pos, normal, w, h)
	if not building then return end
	local n = normal:GetNormalized()
	local right = n:Angle():Right() * (w / 2)
	local up = n:Angle():Up() * (h / 2)
	mesh.Quad(pos - right + up, pos + right + up, pos + right - up, pos - right - up)
end

-- require / module: GMod's includes/modules/<name>.lua; binary modules are not a thing here
local required = {}
function require(name)
	if required[name] ~= nil then return required[name] end
	-- actors are single packages: a module is a table `module(name)` registered in this realm
	local t = _G[name]
	if type(t) == "table" and t._NAME == name then required[name] = t return t end
	error("Module not found: " .. tostring(name), 2)
end
function module(name, ...)
	local t = _G[name]
	if type(t) ~= "table" then t = {}; _G[name] = t end
	t._NAME = name
	for i = 1, select("#", ...) do local f = select(i, ...) if type(f) == "function" then f(t) end end
	return t
end
package = package or {}
function package.seeall(t) setmetatable(t, { __index = _G }) end

-- control presets (settings/presets/<name>.txt in the game): stored in the addon store
local presets_cache
local function presets_load()
	if presets_cache then return presets_cache end
	presets_cache = {}
	if file and file.Read then local raw = file.Read("presets.json", "DATA") if raw then presets_cache = util.JSONToTable(raw) or {} end end
	return presets_cache
end
function LoadPresets() return presets_load() end
function SavePresets(p) presets_cache = p or {} if file and file.Write then file.Write("presets.json", util.TableToJSON(presets_cache)) end end

-- spawn icon camera for a model: the game frames the model's bounds (open: no model bounds without the host)
function PositionSpawnIcon(model, pos, noAngles)
	local mins, maxs = model.GetRenderBounds and model:GetRenderBounds() or Vector(-16, -16, -16), Vector(16, 16, 16)
	if type(mins) ~= "table" then mins, maxs = Vector(-16, -16, -16), Vector(16, 16, 16) end
	local size = math.max(maxs.x - mins.x, maxs.y - mins.y, maxs.z - mins.z, 1)
	local center = (mins + maxs) / 2
	local ang = noAngles and Angle(0, 0, 0) or Angle(25, 220, 0)
	local dist = size * 1.5
	return { origin = center - ang:Forward() * dist, angles = ang, fov = 45, znear = 1, zfar = dist * 4 }
end
function OnModelLoaded(model, nummeshes, numtris, numverts, numbones, numbodyparts, numvertices) end
function AddPropsOfParent(pnlContent, node, parentid, customProps)
	local props = customProps or spawnmenu.GetPropTable()
	for cat, v in SortedPairs(props) do
		if v.parentid == parentid then
			local n = node:AddNode(v.name or cat, v.icon)
			n.DoPopulate = function(self)
				if self.PropPanel then return end
				self.PropPanel = vgui.Create("ContentContainer", pnlContent)
				self.PropPanel:SetVisible(false)
				for _, c in ipairs(v.contents or {}) do spawnmenu.CreateContentIcon(c.type or "model", self.PropPanel, c) end
			end
			n.DoClick = function(self) self:DoPopulate(); pnlContent:SwitchPanel(self.PropPanel) end
			AddPropsOfParent(pnlContent, n, v.id or cat, customProps)
		end
	end
end
local saved_cursor
function RememberCursorPosition() saved_cursor = { gui.MousePos() } end
function RestoreCursorPosition() if saved_cursor then gui.SetMousePos(saved_cursor[1], saved_cursor[2]) end end
for _, n in ipairs({ "RenderDoF", "RenderStereoscopy", "RenderSuperDoF", "JS_Language", "JS_Utility", "JS_Workshop", "WorkshopFileBase", "VisualizeLayout" }) do
	_G[n] = stub(n)
end

-- ---------------------------------------------------------------------------------------------
-- sound.PlayFile / IGModAudioChannel: sample-backed channels on the host mixer. The channel
-- id goes out as an audio command; the host reports state/time/length back each frame.
-- URL streaming has no decoder here - PlayURL fails with BASS_ERROR_NOTAVAIL (open).
-- ---------------------------------------------------------------------------------------------
local channels = {}
local next_channel = 0
local CHAN = {}
CHAN.__index = CHAN
CHAN.MetaName = "IGModAudioChannel"
local bass_errors = { [2] = "BASS_ERROR_FILEOPEN", [37] = "BASS_ERROR_NOTAVAIL", [41] = "BASS_ERROR_FILEFORM" }
local function audio(c, op, t) hostdraw.audio(c.__id, op, t or {}) end
local function new_channel(path, flags, cb)
	next_channel = next_channel + 1
	local c = setmetatable({
		__id = next_channel, __file = path, __cb = cb, __valid = true, __ready = false,
		__volume = 1, __rate = 1, __time = 0, __length = 0, __hz = 0, __loop = false,
		__3d = flags:find("3d") ~= nil, __pos = Vector(), __fade = { 200, 1000000000 }, __cone = { 360, 360, 1 }, __pan = 0,
		__paused = flags:find("noplay") ~= nil, __stopped = false,
	}, CHAN)
	c.__state = c.__paused and GMOD_CHANNEL_PAUSED or GMOD_CHANNEL_STALLED
	channels[c.__id] = c
	return c
end
local function start(c)
	audio(c, "play", { path = c.__file, volume = c.__volume, pitch = c.__rate * 100, looping = c.__loop, paused = c.__paused, pos = c.__3d and c.__pos or nil })
end
function sound.PlayFile(path, flags, cb)
	local c = new_channel(tostring(path), tostring(flags or ""), cb)
	start(c)
end
function sound.PlayURL(url, flags, cb) if cb then cb(nil, 37, bass_errors[37]) end end
events.on("audio.state", function(p)
	local c = channels[p.id]
	if not c then return end
	if p.err ~= 0 then
		channels[p.id] = nil
		c.__valid = false
		if c.__cb then local cb = c.__cb c.__cb = nil cb(nil, p.err, bass_errors[p.err] or "BASS_ERROR_UNKNOWN") end
		return
	end
	if p.state == GMOD_CHANNEL_STALLED then return end
	c.__time, c.__hz = p.time, p.rate
	if p.length > 0 then c.__length = p.length end
	if c.__stopped then c.__state = GMOD_CHANNEL_STOPPED else c.__state = p.state end
	if not c.__ready then
		c.__ready = true
		if c.__cb then local cb = c.__cb c.__cb = nil cb(c, 0, nil) end
	end
end)
function CHAN:IsValid() return self.__valid == true end
function CHAN:__tostring() return "IGModAudioChannel: " .. self.__file end
function CHAN:GetFileName() return self.__file end
function CHAN:Play()
	if not self.__valid then return end
	if self.__stopped or self.__state == GMOD_CHANNEL_STOPPED then self.__stopped, self.__paused = false, false; self.__time = 0; start(self) return end
	self.__paused = false; self.__state = GMOD_CHANNEL_PLAYING; audio(self, "pause", { paused = false })
end
function CHAN:Pause() if not self.__valid then return end self.__paused = true; self.__state = GMOD_CHANNEL_PAUSED; audio(self, "pause", { paused = true }) end
function CHAN:Stop() if not self.__valid then return end self.__stopped = true; self.__state = GMOD_CHANNEL_STOPPED; self.__time = 0; audio(self, "stop") end
function CHAN:SetVolume(v) self.__volume = math.max(tonumber(v) or 1, 0); audio(self, "volume", { volume = self.__volume }) end
function CHAN:GetVolume() return self.__volume end
function CHAN:SetPlaybackRate(r) self.__rate = math.max(tonumber(r) or 1, 0); audio(self, "pitch", { pitch = self.__rate * 100 }) end
function CHAN:GetPlaybackRate() return self.__rate end
function CHAN:SetTime(t, dont_decode) self.__time = math.max(tonumber(t) or 0, 0); audio(self, "seek", { time = self.__time }) end
function CHAN:GetTime() return self.__time end
function CHAN:GetLength() return self.__length end
function CHAN:GetState() return self.__state end
function CHAN:EnableLooping(b) self.__loop = b == true; audio(self, "loop", { looping = self.__loop }) end
function CHAN:IsLooping() return self.__loop end
function CHAN:Is3D() return self.__3d end
function CHAN:Set3DEnabled(b) self.__3d = b == true; audio(self, "pos", { pos = self.__3d and self.__pos or nil }) end
function CHAN:Get3DEnabled() return self.__3d end
function CHAN:SetPos(pos, dir) self.__pos = pos; self.__dir = dir; if self.__3d then audio(self, "pos", { pos = pos }) end end
function CHAN:GetPos() return self.__pos, self.__dir or Vector() end
function CHAN:Set3DFadeDistance(min, max) self.__fade = { min, max } end
function CHAN:Get3DFadeDistance() return self.__fade[1], self.__fade[2] end
function CHAN:Set3DCone(inner, outer, vol) self.__cone = { inner, outer, vol } end
function CHAN:Get3DCone() return self.__cone[1], self.__cone[2], self.__cone[3] end
function CHAN:SetPan(p) self.__pan = math.Clamp(tonumber(p) or 0, -1, 1) end
function CHAN:GetPan() return self.__pan end
function CHAN:GetSamplingRate() return self.__hz end
function CHAN:GetBitsPerSample() return 16 end
function CHAN:GetAverageBitRate() return self.__hz * 16 / 1000 end
function CHAN:GetBufferedTime() return self.__length end
function CHAN:IsBlockStreamed() return false end
function CHAN:IsOnline() return false end
function CHAN:GetLevel() return 0, 0 end
function CHAN:FFT(tbl, size) return 0 end
function CHAN:GetTagsHTTP() return nil end
function CHAN:GetTagsICY() return nil end
function CHAN:GetTagsID3() return nil end
function CHAN:GetTagsMP4() return nil end
function CHAN:GetTagsMeta() return nil end
function CHAN:GetTagsOGG() return nil end
function CHAN:GetTagsVendor() return nil end
function CHAN:GetTagsWMA() return nil end
__glua_classes.IGModAudioChannel = CHAN

-- ---------------------------------------------------------------------------------------------
-- menu realm: the game's main-menu Lua surface. Local state lives in cookies (presets,
-- disabled addons, last map, language, background images, problems); servers go through the
-- host's browser (`serverlist`); the HTML-menu JS bridges and the Steam-only calls are stubs.
-- ---------------------------------------------------------------------------------------------
local function json_cookie(key, default) local raw = cookie.GetString(key, nil) if raw == nil or raw == "" then return default end return util.JSONToTable(raw) or default end
local function set_json_cookie(key, t) cookie.Set(key, util.TableToJSON(t)) end

-- addons
local function disabled_set() return json_cookie("menu.addons.disabled", {}) end
function GetAddonStatus()
	local all, disabled = engine.GetAddons(), disabled_set()
	local s = { total = #all, mounted = 0, disabled = 0 }
	for _, a in ipairs(all) do if disabled[a.wsid] or disabled[a.file] then s.disabled = s.disabled + 1 elseif a.mounted then s.mounted = s.mounted + 1 end end
	return s
end
function MenuGetAddonData(wsid) for _, a in ipairs(engine.GetAddons()) do if a.wsid == wsid or a.file == wsid then return a end end end
function UpdateAddonDisabledState() local d = disabled_set() for _, a in ipairs(engine.GetAddons()) do a.mounted = not (d[a.wsid] or d[a.file]) end end
function UpdateSubscribedAddon(wsid) end
function UpdateAddonMapList() end
function RefreshAddonConflicts() end
function FireAddonConflicts(json) end
-- addon presets: name -> { enabled = {wsid...}, disabled = {...}, newAction = ... }
function ListAddonPresets() local names = {} for n in SortedPairs(json_cookie("menu.addons.presets", {})) do names[#names + 1] = n end return names end
function LoadAddonPresets() return json_cookie("menu.addons.presets", {}) end
function SaveAddonPresets(json) if type(json) == "string" then json = util.JSONToTable(json) end set_json_cookie("menu.addons.presets", json or {}) end
function CreateNewAddonPreset(json)
	local p = type(json) == "string" and util.JSONToTable(json) or json
	if not p or not p.name then return end
	local all = LoadAddonPresets(); all[p.name] = p; set_json_cookie("menu.addons.presets", all)
end
function DeleteAddonPreset(name) local all = LoadAddonPresets(); all[name] = nil; set_json_cookie("menu.addons.presets", all) end
-- background images
local backgrounds, background = {}, nil
function AddBackgroundImage(path) backgrounds[#backgrounds + 1] = path end
function ClearBackgroundImages() backgrounds, background = {}, nil end
function ChangeBackground(kind) if #backgrounds > 0 then background = backgrounds[math.random(#backgrounds)] end return background end
-- problems (the menu's problems panel)
local problems = {}
function FireProblem(t) if type(t) == "table" and t.id then problems[t.id] = t; hook.Run("OnProblemAdded", t) end end
function FireProblemFromEngine(id, severity, params) FireProblem({ id = id, severity = severity, params = params, type = "engine" }) end
function ClearProblem(id) if problems[id] then problems[id] = nil; hook.Run("OnProblemCleared", id) end end
function ClearLuaErrorGroup(group) for id, p in pairs(problems) do if p.type == "lua" and p.group == group then problems[id] = nil end end end
function OpenProblemsPanel()
	local f = vgui.Create("DFrame"); f:SetTitle("#problems.title"); f:SetSize(500, 400); f:Center(); f:MakePopup()
	local list = vgui.Create("DListView", f); list:Dock(FILL); list:AddColumn("#problems.severity"); list:AddColumn("#problems.id")
	for id, p in SortedPairs(problems) do list:AddLine(tostring(p.severity or 0), tostring(id)) end
	return f
end
-- loading / game state
function IsInGame() return IsValid(LocalPlayer()) end
function IsInLoading() return false end
function GetLoadStatus() return IsInGame() and "" or "Not connected" end
local load_panel
function GetLoadPanel() if not load_panel or not load_panel.__valid then load_panel = vgui.Create("DPanel"); load_panel:SetVisible(false) end return load_panel end
function UpdateLoadPanel(js) end
function CancelLoading() RunConsoleCommand("disconnect") end
function GetDemoFileDetails(path) return nil end
function GetSaveFileDetails(path) return nil end
function RecordDemoFrame() end
function GetDownloadables() return {} end
function NumDownloadables() return 0 end
function TranslateDownloadableName(name) return name end
function GetDefaultLoadingHTML() return "" end
function GetAPIManifest() return "" end
function LoadNewsList() end
function SaveHideNews(hide) cookie.Set("menu.news.hide", hide and "1" or "0") end
function UpdateGames() end
function UpdateLanguages() end
function UpdateMapList() end
function UpdateServerSettings() end
-- language / maps
function LanguageChanged(lang) cookie.Set("gmod_language", lang); RunConsoleCommand("gmod_language", lang) end
function GetMapList() local maps = file.Find("maps/*.bsp", "GAME") or {} for i, m in ipairs(maps) do maps[i] = string.StripExtension(m) end return maps end
function SaveLastMap(map, category) cookie.Set("menu.lastmap", map); cookie.Set("menu.lastmap.category", category or "") end
function LoadLastMap() local m = cookie.GetString("menu.lastmap", "") if m ~= "" then RunConsoleCommand("map", m) end end
-- servers (through serverlist / the host browser)
local menu_servers = {}
function GetServers(kind, id)
	menu_servers[kind] = {}
	serverlist.Query({ Type = kind, Callback = function(ping, name, desc, map, players, maxplayers, bots, pass, lastplayed, address, gamemode)
		menu_servers[kind][#menu_servers[kind] + 1] = { ping = ping, name = name, map = map, players = players, maxplayers = maxplayers, address = address, gamemode = gamemode, lastplayed = lastplayed }
	end })
	return id
end
function DoStopServers(kind) end
function CanAddServerToFavorites() return true end
function IsServerBlacklisted(address, hostname, description, gamemode, map) return false end
function ToggleFavourite(address) if serverlist.IsServerFavorite(address) then serverlist.RemoveServerFromFavorites(address) else serverlist.AddServerToFavorites(address) end end
function JoinServer(address) RunConsoleCommand("connect", address) end
function RequestConnectToServer(address) permissions.AskToConnect(address) end
function GetPlayerList(address) serverlist.PlayerList(address, function() end) end
local game_details
function GameDetails(servername, serverurl, mapname, maxplayers, steamid, gamemode) game_details = { servername = servername, serverurl = serverurl, mapname = mapname, maxplayers = maxplayers, steamid = steamid, gamemode = gamemode }; hook.Run("GameDetails", game_details) end
function GMOD_OpenURLNoOverlay(url) gui.OpenURL(url) end
function RequestOpenURL(url) gui.OpenURL(url) end
function OpenFolder(path) gui.OpenURL("file://" .. tostring(path)) end
function RunGameUICommand(cmd)
	cmd = tostring(cmd)
	if cmd:sub(1, 7) == "engine " then RunConsoleCommand(unpack(string.Explode(" ", cmd:sub(8))))
	elseif cmd == "Quit" or cmd == "QuitNoConfirm" then RunConsoleCommand("quit")
	elseif cmd == "ResumeGame" then if gui.HideGameUI then gui.HideGameUI() end
	elseif cmd == "Disconnect" then RunConsoleCommand("disconnect")
	end
end
function RequestPermission(name) return permissions.IsGranted(name) end

-- permissions: what the player lets servers do; stored per name
permissions = permissions or {}
local permission_list = {
	{ name = "connect", desc = "Connect to a server" },
	{ name = "voicerecord", desc = "Record voice chat" },
}
local function grants() return json_cookie("menu.permissions", {}) end
function permissions.GetAll() local out = {} for i, p in ipairs(permission_list) do out[i] = { name = p.name, desc = p.desc, granted = grants()[p.name] == true } end return out end
function permissions.IsGranted(name) return grants()[name] == true end
function permissions.Grant(name, granted) local g = grants(); g[name] = granted ~= false and true or nil; set_json_cookie("menu.permissions", g); hook.Run("PermissionsChanged") end
function permissions.Revoke(name) permissions.Grant(name, false) end
function permissions.Connect(address) RunConsoleCommand("connect", address) end
function permissions.AskToConnect(address)
	if permissions.IsGranted("connect") then permissions.Connect(address) return end
	Derma_Query("#permissions.connect.ask " .. tostring(address), "#permissions.connect.title", "#permissions.yes", function() permissions.Connect(address) end, "#permissions.no")
end
function permissions.EnableVoiceChat(enable) RunConsoleCommand("voice_modenable", enable and "1" or "0") end

-- serverlist: queries and pings run on the host's server browser, entries come back per frame
serverlist = serverlist or {}
local queries, next_query, favorites = {}, 0, nil
local function servers_op(id, op, t) hostdraw.servers(id, op, t or {}) end
local function entry_args(p)
	return p.ping, p.name, "", p.map, p.players, p.maxplayers, 0, false, p.lastplayed, p.address, p.gamemode, "", false, "", "", ""
end
function serverlist.Query(data)
	next_query = next_query + 1
	queries[next_query] = { kind = "query", data = data }
	servers_op(next_query, "query", { type = data.Type or "internet" })
end
function serverlist.PingServer(address, cb)
	next_query = next_query + 1
	queries[next_query] = { kind = "ping", cb = cb }
	servers_op(next_query, "ping", { address = address })
end
function serverlist.PlayerList(address, cb) end
local function load_favorites() if favorites then return end favorites = {}; next_query = next_query + 1; queries[next_query] = { kind = "favorites" }; servers_op(next_query, "favorites") end
function serverlist.IsServerFavorite(address) load_favorites() return favorites[address] == true end
function serverlist.AddServerToFavorites(address) load_favorites(); favorites[address] = true; servers_op(0, "favorite_add", { address = address }) end
function serverlist.RemoveServerFromFavorites(address) load_favorites(); favorites[address] = nil; servers_op(0, "favorite_remove", { address = address }) end
local function current_address() return game.GetIPAddress and game.GetIPAddress() or "" end
function serverlist.IsCurrentServerFavorite() return serverlist.IsServerFavorite(current_address()) end
function serverlist.AddCurrentServerToFavorites() serverlist.AddServerToFavorites(current_address()) end
events.on("servers.entry", function(p)
	local q = queries[p.id]
	if not q then return end
	if q.kind == "query" and q.data.Callback then q.data.Callback(entry_args(p))
	elseif q.kind == "ping" and q.cb then q.cb(entry_args(p))
	elseif q.kind == "favorites" then favorites[p.address] = true end
end)
events.on("servers.done", function(p)
	local q = queries[p.id]
	if not q then return end
	queries[p.id] = nil
	if q.kind == "query" and q.data.Finished then q.data.Finished() end
end)

-- Steam-only (no Steam here; our workshop is the launcher's), the HTML-menu bridges, demos
steamworks = steamworks or {}
for _, n in ipairs({ "ApplyAddons", "Download", "DownloadUGC", "FileInfo", "GetList", "GetPlayerName", "IsSubscribed", "OpenWorkshop", "Publish", "RequestPlayerInfo", "SetFileCompleted", "SetFilePlayed", "SetShouldMountAddon", "ShouldMountAddon", "Subscribe", "Unsubscribe", "ViewFile", "Vote", "VoteInfo" }) do
	steamworks[n] = stub("steamworks." .. n)
end
menu = menu or {}
menu.RecordFrame = stub("menu.RecordFrame")
engine.OpenDupe = stub("engine.OpenDupe")
engine.WriteDupe = stub("engine.WriteDupe")
engine.WriteSave = stub("engine.WriteSave")
function engine.VideoSettings() return { windowed = true, width = ScrW(), height = ScrH(), vsync = 1, fullscreen = false } end

-- menubar: the spawnmenu's top bar
menubar = menubar or {}
local menubar_panel
function menubar.Init() menubar_panel = vgui.Create("DMenuBar"); menubar_panel:SetVisible(false); hook.Run("PopulateMenuBar", menubar_panel); return menubar_panel end
function menubar.ParentTo(pnl) if not menubar_panel or not menubar_panel.__valid then menubar.Init() end menubar_panel:SetParent(pnl); menubar_panel:SetVisible(true); menubar_panel:Dock(TOP) end
function menubar.IsParent(pnl) return menubar_panel ~= nil and menubar_panel.__valid and menubar_panel.parent == pnl end

-- ---------------------------------------------------------------------------------------------
-- remaining client libraries
-- ---------------------------------------------------------------------------------------------
-- render: the engine's named textures as ITexture objects, a model draw, misc state
for _, n in ipairs({ "GetBloomTex0", "GetBloomTex1", "GetMoBlurTex0", "GetMoBlurTex1", "GetMorphTex0", "GetMorphTex1", "GetSmallTex0", "GetSmallTex1", "GetSuperFPTex", "GetSuperFPTex2", "GetPowerOfTwoTexture", "GetRefractTexture", "GetFullScreenDepthTexture", "GetResolvedFullFrameDepth" }) do
	local tex_name = "_rt_" .. string.gsub(string.gsub(n, "^Get", ""), "Texture$", "")
	render[n] = function() return GetRenderTargetEx(tex_name, ScrW(), ScrH(), RT_SIZE_FULL_FRAME_BUFFER, MATERIAL_RT_DEPTH_SHARED, 0, 0, IMAGE_FORMAT_RGBA8888) end
end
local fog_max_density, world_override = 1, nil
function render.GetFogMaxDensity() return fog_max_density end
local set_fog_max = render.SetFogMaxDensity
function render.SetFogMaxDensity(d) fog_max_density = d if set_fog_max then set_fog_max(d) end end
function render.GetHDREnabled() return false end
function render.IsTakingScreenshot() return false end
function render.TurnOnToneMapping() end
function render.BindLocalCubemap(tex) end
function render.SetLightmapTexture(tex) end
function render.WorldMaterialOverride(mat) world_override = mat end
-- no lightmap sampling on the client yet: a lit white surface
function render.GetSurfaceColor(a, b) return Vector(1, 1, 1) end
function render.Model(settings, ent)
	if not settings or not settings.model then return end
	hostdraw.model(settings.model, settings.pos or Vector(), __glua_ang_xyz(settings.angle or Angle()), 255)
end

-- util: the world picker, visibility (no occlusion queries: a point in view is fully visible),
-- the menu path helpers
local picker = { active = false, cb = nil }
util.worldpicker = setmetatable({}, { __call = function(_, cb) util.worldpicker.Start(cb) end })
function util.worldpicker.Active() return picker.active end
function util.worldpicker.Start(cb) picker.active, picker.cb = true, cb; gui.EnableScreenClicker(true) end
function util.worldpicker.Finish(tr)
	picker.active = false
	gui.EnableScreenClicker(false)
	local cb = picker.cb
	picker.cb = nil
	if cb then cb(tr) end
end
events.on("input.key", function(p)
	if not picker.active or not p.down or p.key ~= "MouseLeft" then return end
	local dir = gui.ScreenToVector(gui.MousePos())
	local tr = util.TraceLine({ start = EyePos(), endpos = EyePos() + dir * 16384, filter = LocalPlayer() })
	util.worldpicker.Finish(tr)
end)
function util.GetPixelVisibleHandle() return { __pixvis = true } end
function util.PixelVisible(pos, radius, handle) local s = pos:ToScreen() return s.visible and 1 or 0 end
function util.GetSunInfo() return { direction = Vector(0.3, 0.3, 0.9):GetNormalized(), obstruction = 0 } end
function util.IsPlayerSpeaking(ply) return false end
function util.RemoveDecalsAt(pos, radius) end
function util.FullPathToRelative_Menu(path, game_dir) local p = string.gsub(path, "\\", "/") local i = string.find(p, "/garrysmod/", 1, true) return i and string.sub(p, i + 11) or p end
function util.RelativePathToFull_Menu(path, game_dir) return (game_dir or "garrysmod") .. "/" .. path end
function util.RelativePathToGMA_Menu(path) return string.gsub(path, "^addons/", "") end

-- os: time from the host clock, dates in UTC
os = os or {}
local function civil(days)
	local z = days + 719468
	local era = math.floor(z / 146097)
	local doe = z - era * 146097
	local yoe = math.floor((doe - math.floor(doe / 1460) + math.floor(doe / 36524) - math.floor(doe / 146096)) / 365)
	local y = yoe + era * 400
	local doy = doe - (365 * yoe + math.floor(yoe / 4) - math.floor(yoe / 100))
	local mp = math.floor((5 * doy + 2) / 153)
	local d = doy - math.floor((153 * mp + 2) / 5) + 1
	local m = mp < 10 and mp + 3 or mp - 9
	if m <= 2 then y = y + 1 end
	return y, m, d, doy
end
local month_names = { "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December" }
local day_names = { "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday" }
local host_time = os.time
function os.time(t)
	if type(t) == "table" then
		local y, m, d = t.year, t.month, t.day
		if m <= 2 then y = y - 1 end
		local era = math.floor(y / 400)
		local yoe = y - era * 400
		local mp = m > 2 and m - 3 or m + 9
		local doy = math.floor((153 * mp + 2) / 5) + d - 1
		local doe = yoe * 365 + math.floor(yoe / 4) - math.floor(yoe / 100) + doy
		local days = era * 146097 + doe - 719468
		return days * 86400 + (t.hour or 12) * 3600 + (t.min or 0) * 60 + (t.sec or 0)
	end
	if host_time then return host_time() end
	return math.floor(__glua_unixtime())
end
os.clock = os.clock or function() return SysTime() end
function os.difftime(a, b) return a - (b or 0) end
function os.date(fmt, t)
	fmt = fmt or "%c"
	t = t or os.time()
	local utc = string.sub(fmt, 1, 1) == "!"
	if utc then fmt = string.sub(fmt, 2) end
	local days = math.floor(t / 86400)
	local secs = t - days * 86400
	local y, m, d, doy = civil(days)
	local H, M, S = math.floor(secs / 3600), math.floor(secs % 3600 / 60), math.floor(secs % 60)
	local wday = (days + 4) % 7 + 1
	local yday = math.floor((t - os.time({ year = y, month = 1, day = 1, hour = 0 })) / 86400) + 1
	if fmt == "*t" then return { year = y, month = m, day = d, hour = H, min = M, sec = S, wday = wday, yday = yday, isdst = false } end
	local map = {
		Y = tostring(y), y = string.format("%02d", y % 100), m = string.format("%02d", m), d = string.format("%02d", d),
		H = string.format("%02d", H), M = string.format("%02d", M), S = string.format("%02d", S), I = string.format("%02d", (H % 12 == 0) and 12 or H % 12),
		p = H < 12 and "AM" or "PM", A = day_names[wday], a = string.sub(day_names[wday], 1, 3), B = month_names[m], b = string.sub(month_names[m], 1, 3),
		j = string.format("%03d", yday), w = tostring(wday - 1), Z = "UTC", z = "+0000", ["%"] = "%",
	}
	map.x = map.m .. "/" .. map.d .. "/" .. map.y
	map.X = map.H .. ":" .. map.M .. ":" .. map.S
	map.c = map.a .. " " .. map.b .. " " .. map.d .. " " .. map.X .. " " .. map.Y
	return (string.gsub(fmt, "%%(.)", function(c) return map[c] or ("%" .. c) end))
end

-- markup: <color=r,g,b,a> / <colour> / <font=name> tags into a drawable object
markup = markup or {}
local MARKUP = {}
MARKUP.__index = MARKUP
function markup.Escape(s) return (string.gsub(string.gsub(string.gsub(tostring(s), "&", "&amp;"), "<", "&lt;"), ">", "&gt;")) end
local function unescape(s) return (string.gsub(string.gsub(string.gsub(s, "&lt;", "<"), "&gt;", ">"), "&amp;", "&")) end
function markup.Color(str)
	local r, g, b, a = string.match(str, "(%d+)%s*,%s*(%d+)%s*,%s*(%d+)%s*,?%s*(%d*)")
	if not r then return Color(255, 255, 255) end
	return Color(tonumber(r), tonumber(g), tonumber(b), tonumber(a) or 255)
end
local function text_w(font, t) surface.SetFont(font) local w, h = surface.GetTextSize(t) if w == 0 then w = #t * 8 end if h == 0 then h = 16 end return w, h end
function markup.Parse(text, maxwidth)
	local color_stack, font_stack = { Color(255, 255, 255) }, { "DermaDefault" }
	local blocks = {}
	local pos = 1
	local function push_text(t)
		if t == "" then return end
		for seg, nl in string.gmatch(t, "([^\n]*)(\n?)") do
			if seg ~= "" then blocks[#blocks + 1] = { text = unescape(seg), color = color_stack[#color_stack], font = font_stack[#font_stack] } end
			if nl == "\n" then blocks[#blocks + 1] = { newline = true } end
			if seg == "" and nl == "" then break end
		end
	end
	while true do
		local a, b, closing, tag, arg = string.find(text, "<(/?)(%a+)=?([^>]*)>", pos)
		if not a then push_text(string.sub(text, pos)) break end
		push_text(string.sub(text, pos, a - 1))
		tag = string.lower(tag)
		if tag == "color" or tag == "colour" then
			if closing == "/" then if #color_stack > 1 then table.remove(color_stack) end else color_stack[#color_stack + 1] = markup.Color(arg) end
		elseif tag == "font" then
			if closing == "/" then if #font_stack > 1 then table.remove(font_stack) end else font_stack[#font_stack + 1] = arg end
		else
			push_text(string.sub(text, a, b))
		end
		pos = b + 1
	end
	-- lay out: lines of segments, wrapped by word at maxwidth
	local lines, line, x, line_h = {}, {}, 0, 0
	local function flush() lines[#lines + 1] = { segs = line, w = x, h = line_h > 0 and line_h or 16 } line, x, line_h = {}, 0, 0 end
	for _, blk in ipairs(blocks) do
		if blk.newline then flush()
		else
			for word, sep in string.gmatch(blk.text, "([^%s]*)(%s*)") do
				if word == "" and sep == "" then break end
				local piece = word .. sep
				local w, h = text_w(blk.font, piece)
				if maxwidth and maxwidth > 0 and x > 0 and x + w > maxwidth then flush() end
				line[#line + 1] = { text = piece, color = blk.color, font = blk.font, x = x, w = w, h = h }
				x = x + w
				if h > line_h then line_h = h end
			end
		end
	end
	if #line > 0 or #lines == 0 then flush() end
	local total_w, total_h = 0, 0
	for _, l in ipairs(lines) do total_w = math.max(total_w, l.w); total_h = total_h + l.h end
	return setmetatable({ __lines = lines, __w = total_w, __h = total_h, __maxw = maxwidth or 0 }, MARKUP)
end
function MARKUP:GetWidth() return self.__w end
function MARKUP:GetHeight() return self.__h end
function MARKUP:GetMaxWidth() return self.__maxw end
function MARKUP:Size() return self.__w, self.__h end
function MARKUP:Draw(x, y, halign, valign, alpha, textalign)
	halign, valign, alpha = halign or TEXT_ALIGN_LEFT, valign or TEXT_ALIGN_TOP, alpha or 255
	if halign == TEXT_ALIGN_CENTER then x = x - self.__w / 2 elseif halign == TEXT_ALIGN_RIGHT then x = x - self.__w end
	if valign == TEXT_ALIGN_CENTER then y = y - self.__h / 2 elseif valign == TEXT_ALIGN_BOTTOM then y = y - self.__h end
	local cy = y
	for _, l in ipairs(self.__lines) do
		local off = 0
		if textalign == TEXT_ALIGN_CENTER then off = (self.__w - l.w) / 2 elseif textalign == TEXT_ALIGN_RIGHT then off = self.__w - l.w end
		for _, seg in ipairs(l.segs) do
			surface.SetFont(seg.font)
			surface.SetTextColor(Color(seg.color.r, seg.color.g, seg.color.b, math.min(seg.color.a or 255, alpha)))
			surface.SetTextPos(x + off + seg.x, cy + (l.h - seg.h))
			surface.DrawText(seg.text)
		end
		cy = cy + l.h
	end
end
__glua_classes.MarkupObject = MARKUP

-- matproxy: material proxies registered by addons (the renderer does not run proxies yet)
matproxy = matproxy or {}
matproxy.ProxyList, matproxy.ActiveList = {}, {}
function matproxy.Add(def) if def and def.name then matproxy.ProxyList[def.name] = def end end
function matproxy.ShouldOverrideProxy(name) return matproxy.ProxyList[name] ~= nil end
function matproxy.Init(name, uname, mat, values)
	local def = matproxy.ProxyList[name]
	if not def then return end
	matproxy.ActiveList[uname] = { def = def, mat = mat, values = values }
	if def.init then def.init(def, mat, values) end
end
function matproxy.Call(uname, mat, ent)
	local a = matproxy.ActiveList[uname]
	if a and a.def.bind then a.def.bind(a.def, mat, ent) end
end

-- motionsensor: no Kinect here
motionsensor = motionsensor or {}
function motionsensor.IsAvailable() return false end
function motionsensor.IsActive() return false end
function motionsensor.Start() return false end
function motionsensor.Stop() end
function motionsensor.GetSkeleton() return nil end
function motionsensor.GetColourMaterial() return Material("__error") end

-- halo: glow passes collected per frame (the client's renderer has no halo pass yet - open)
halo = halo or {}
local halos, halo_ent = {}, nil
function halo.Add(ents, color, blurx, blury, passes, additive, ignorez) halos[#halos + 1] = { ents = ents, color = color, blurx = blurx or 2, blury = blury or 2, passes = passes or 1, additive = additive ~= false, ignorez = ignorez == true } end
function halo.RenderedEntity() return halo_ent end
function halo.Render(pnl) halos = {} end
hook.Add("PostDrawEffects", "__glua_halo_clear", function() halos = {} end)

-- cam: the view and projection as VMatrix
local function basis_matrix(pos, ang)
	local m = Matrix()
	m:SetTranslation(pos)
	m:SetAngles(ang)
	return m
end
function cam.GetViewMatrix() local m = basis_matrix(EyePos(), EyeAngles()) return m:GetInverse() or m end
function cam.GetProjectionMatrix()
	local lp = LocalPlayer()
	local fov = IsValid(lp) and lp:GetFOV() or 90
	local aspect = ScrW() / math.max(ScrH(), 1)
	local f = 1 / math.tan(math.rad(fov) / 2)
	local n, fz = 3, 28377
	return Matrix({ { f / aspect, 0, 0, 0 }, { 0, f, 0, 0 }, { 0, 0, (fz + n) / (n - fz), 2 * fz * n / (n - fz) }, { 0, 0, -1, 0 } })
end
function cam.GetModelMatrix() return __glua_model_matrix or Matrix() end
function cam.ApplyShake(pos, ang, factor) return pos, ang end

-- surface: paint state, scissor, textures
function surface.GetPanelPaintState() local x, y = surface.__origin_get and surface.__origin_get() or 0, 0 return { translate_x = x, translate_y = y, scissor_enabled = false, scissor_left = 0, scissor_top = 0, scissor_right = ScrW(), scissor_bottom = ScrH() } end
function surface.GetScissorRect() return 0, 0, ScrW(), ScrH() end
function surface.GetTextureNameByID(id) local m = type(id) == "table" and id or nil return m and m.__name or "" end
function surface.GetTextureSize(id) local m = type(id) == "table" and id or nil return m and m.__w or 32, m and m.__h or 32 end

-- system / vgui / language
function system.FlashWindow() end
function system.IsWindowed() return engine.VideoSettings().windowed end
function language.FormatPhrase(token, ...) local ok, s = pcall(string.format, language.GetPhrase(token), ...) return ok and s or language.GetPhrase(token) end

-- client effects: util.Effect runs a registered EFFECT (Init, Think until false, Render each frame)
local active_effects = {}
function util.Effect(name, data, allowOverride, ignorePrediction)
	local def = effects.__registry and effects.__registry[name]
	if not def then return end
	local e = setmetatable({ __valid = true, __class = name }, { __index = def })
	e.IsValid = function(s) return s.__valid end
	e.Remove = function(s) s.__valid = false end
	if e.Init then e:Init(data) end
	active_effects[#active_effects + 1] = e
end
function __glua_run_effects()
	for i = #active_effects, 1, -1 do
		local e = active_effects[i]
		if not e.__valid or (e.Think and e:Think() == false) then
			e.__valid = false
			table.remove(active_effects, i)
		elseif e.Render then
			e:Render()
		end
	end
end
-- @hook EFFECT:Init @hook EFFECT:Think @hook EFFECT:Render
hook.Add("PostDrawTranslucentRenderables", "__glua_effects", function() __glua_run_effects() end)

-- particle systems, client ropes, projected texture textures
local CNP = __glua_classes.CNewParticleEffect
if CNP then
	function CNP:SetShouldSimulate(b) self.__simulate = b ~= false end
	function CNP:GetShouldSimulate() return self.__simulate ~= false end
end
__glua_classes.ProjectedTexture = PT
function PT:SetTexture(t) self.__texture = t end
function PT:GetTexture() return self.__texture end
function ents.CreateClientRope(startEnt, startBone, endEnt, endBone, startPos, endPos, width, mat, extra)
	local rope = ClientsideModel("")
	rope.__rope = { startEnt = startEnt, startBone = startBone, endEnt = endEnt, endBone = endBone, startPos = startPos, endPos = endPos, width = width or 1, mat = mat or "cable/cable2", extra = extra }
	return rope
end

-- LuaJIT internals have no equivalent in this VM; video encoding and the widget renderer are open
jit = jit or {}
jit.util = jit.util or {}
jit.opt = jit.opt or {}
for _, n in ipairs({ "funcbc", "funcinfo", "funck", "funcuvname", "ircalladdr", "traceexitstub", "traceinfo", "traceir", "tracek", "tracemc", "tracesnap" }) do jit.util[n] = stub("jit.util." .. n) end
jit.opt.start = stub("jit.opt.start")
widgets = widgets or {}
widgets.RenderMe = stub("widgets.RenderMe")
video = video or {}
video.Record = stub("video.Record")

-- branch, clipboard, environments, workshop refresh, effect registry listing
local clipboard = ""
function SetClipboardText(t) clipboard = tostring(t) end
function __glua_clipboard() return clipboard end
getfenv = getfenv or function(f) return _G end
setfenv = setfenv or stub("setfenv")
function UpdateSubscribedAddons() end
function effects.GetList() local out = {} for name in pairs(effects.__registry or {}) do out[#out + 1] = name end table.sort(out) return out end
function effects.TracerSound(start, endpos, kind) end
