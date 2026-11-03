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

function S.DrawRect(x, y, w, h) hostdraw.rect(x, y, w, h, col(draw_color)) end
function S.DrawOutlinedRect(x, y, w, h, t)
	t = t or 1
	local c = col(draw_color)
	hostdraw.rect(x, y, w, t, c)
	hostdraw.rect(x, y + h - t, w, t, c)
	hostdraw.rect(x, y, t, h, c)
	hostdraw.rect(x + w - t, y, t, h, c)
end
-- a line is drawn as the thin rectangle between its ends; a diagonal one as a chain of
-- one-pixel steps (the client has no line primitive in the command list)
function S.DrawLine(x1, y1, x2, y2)
	local c = col(draw_color)
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
	hostdraw.text(text_x, text_y, tostring(text), col(text_color), font)
	local w = hostdraw.text_size(tostring(text), font)
	text_x = text_x + (w or 0)
end

function S.SetMaterial(m) material = m end
function S.SetTexture(id) material = type(id) == "table" and id or nil end
function S.GetTextureID(name) return Material(name) end
function S.GetHUDTexture(name) return Material(name) end
function S.DrawTexturedRect(x, y, w, h)
	if material and material.__name ~= "" then
		hostdraw.texture(x, y, w, h, material.__name, col(draw_color))
	else
		hostdraw.rect(x, y, w, h, col(draw_color))
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
		for i = 1, #xs - 1, 2 do hostdraw.rect(xs[i], y, xs[i + 1] - xs[i], 1, c) end
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

------------------------------------------------------------------------ the frame
-- the game's HUD frame: `PreDrawHUD`, then `HUDPaintBackground` and `HUDPaint` unless
-- `HUDShouldDraw("CHudGMod")` vetoes, then `PostDrawHUD` and `DrawOverlay`; each hook runs
-- with the surface state reset (white, DermaDefault, no material)
if events then
	events.on("render.hud", function(p)
		scr_w, scr_h = p.w or scr_w, p.h or scr_h
		set_color(draw_color, 255, 255, 255, 255)
		set_color(text_color, 255, 255, 255, 255)
		font, material, alpha_mul = "DermaDefault", nil, 1
		hook.Run("PreDrawHUD")
		if hook.Run("HUDShouldDraw", "CHudGMod") ~= false then
			hook.Run("HUDPaintBackground")
			hook.Run("HUDPaint")
		end
		hook.Run("PostDrawHUD")
		hook.Run("DrawOverlay")
	end)
end
