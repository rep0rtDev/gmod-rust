-- GLua vgui / Derma over the client drawing surface (own implementation from the documented
-- behaviour): a panel tree with docking, z order, hover / click / keyboard routing from the
-- client's cursor and key edges, painted each frame after `HUDPaint` through `surface.*`
-- in panel coordinates; the default skin's colours (docs/spec/ui.md). Server realm: stubs.
local stub = __glua_stub

if not CLIENT then
	vgui = vgui or {}
	for _, n in ipairs({ "Create", "CreateFromTable", "Register", "RegisterTable", "GetControlTable", "GetWorldPanel", "GetHoveredPanel", "CursorVisible", "IsHoveringWorld", "GetKeyboardFocus", "FocusedHasParent", "GetAll" }) do vgui[n] = stub("vgui." .. n) end
	derma = derma or {}
	for _, n in ipairs({ "DefineControl", "GetControlList", "Color", "SkinHook", "SkinTexture", "GetDefaultSkin", "GetSkinTable", "GetNamedSkin", "RefreshSkins", "DefineSkin", "SkinChangeIndex", "SkinList" }) do derma[n] = stub("derma." .. n) end
	for _, n in ipairs({ "DermaMenu", "Derma_Message", "Derma_Query", "Derma_StringRequest", "Derma_Anim", "Derma_DrawBackgroundBlur", "Derma_Hook", "Derma_Install_Convar_Functions", "CloseDermaMenus", "RegisterDermaMenuForClose", "DermaSkin", "SetDermaSkin" }) do _G[n] = stub("Global." .. n) end
	return
end

MOUSE_LEFT, MOUSE_RIGHT, MOUSE_MIDDLE, MOUSE_4, MOUSE_5, MOUSE_WHEEL_UP, MOUSE_WHEEL_DOWN = 107, 108, 109, 110, 111, 112, 113
NODOCK, FILL, LEFT, RIGHT, TOP, BOTTOM = 0, 1, 2, 3, 4, 5

------------------------------------------------------------------------ skin (default.lua colours)
local SKIN = {
	bg_color = Color(101, 100, 105), bg_color_dark = Color(55, 57, 61), bg_color_bright = Color(220, 220, 220),
	control_color = Color(120, 120, 120), control_color_highlight = Color(150, 150, 150), control_color_active = Color(110, 150, 250),
	control_color_bright = Color(255, 200, 100), control_color_dark = Color(100, 100, 100),
	colPropertySheet = Color(170, 170, 170), colTab = Color(170, 170, 170), colTabInactive = Color(140, 140, 140),
	colTabShadow = Color(0, 0, 0, 170), colTabText = Color(255, 255, 255), colTabTextInactive = Color(0, 0, 0, 200), fontTab = "DermaDefault",
	colCollapsibleCategory = Color(255, 255, 255, 20), colCategoryText = Color(255, 255, 255), colCategoryTextInactive = Color(200, 200, 200), fontCategoryHeader = "TabLarge",
	colNumSliderNotch = Color(0, 0, 0, 100), colTextEntryText = Color(20, 20, 20), colTextEntryTextHighlight = Color(20, 200, 250),
	colTextEntryTextCursor = Color(0, 0, 100), colTextEntryTextPlaceholder = Color(128, 128, 128),
	colMenuBG = Color(255, 255, 255, 200), colMenuBorder = Color(0, 0, 0, 200),
	colButtonText = Color(255, 255, 255), colButtonTextDisabled = Color(255, 255, 255, 55), colButtonBorder = Color(20, 20, 20),
	colButtonBorderHighlight = Color(255, 255, 255, 50), colButtonBorderShadow = Color(0, 0, 0, 100),
	Colours = {
		Window = { TitleActive = Color(255, 255, 255), TitleInactive = Color(255, 255, 255) },
		Button = { Normal = Color(88, 88, 88), Hover = Color(255, 255, 255), Down = Color(255, 255, 255), Disabled = Color(136, 136, 136) },
		Tab = { Active = { Normal = Color(88, 88, 88), Hover = Color(255, 255, 255), Down = Color(255, 255, 255), Disabled = Color(136, 136, 136) }, Inactive = { Normal = Color(88, 88, 88), Hover = Color(255, 255, 255), Down = Color(255, 255, 255), Disabled = Color(136, 136, 136) } },
		Label = { Default = Color(0, 0, 0), Bright = Color(255, 255, 255), Dark = Color(0, 0, 0), Highlight = Color(255, 0, 0) },
		Tree = { Lines = Color(255, 255, 255), Normal = Color(0, 0, 0), Hover = Color(255, 0, 0), Selected = Color(0, 0, 255) },
		Properties = { Line_Normal = Color(255, 255, 255), Line_Selected = Color(255, 0, 0), Line_Hover = Color(0, 255, 0), Title = Color(0, 0, 0), Column_Normal = Color(255, 255, 255), Column_Selected = Color(255, 0, 0), Column_Hover = Color(0, 255, 0), Border = Color(0, 0, 0), Label_Normal = Color(0, 0, 0), Label_Selected = Color(255, 0, 0), Label_Hover = Color(0, 255, 0) },
		Category = { Header = Color(255, 255, 255), Header_Closed = Color(255, 255, 255), Line = { Text = Color(0, 0, 0), Text_Hover = Color(255, 0, 0), Text_Selected = Color(0, 0, 255), Button = Color(255, 255, 255), Button_Hover = Color(255, 0, 0), Button_Selected = Color(0, 0, 255) }, LineAlt = { Text = Color(0, 0, 0), Text_Hover = Color(255, 0, 0), Text_Selected = Color(0, 0, 255), Button = Color(240, 240, 240), Button_Hover = Color(255, 0, 0), Button_Selected = Color(0, 0, 255) } },
		Tooltip = { Background = Color(255, 255, 255), Text = Color(0, 0, 0) },
		Tab_Control = { Normal = Color(255, 255, 255) },
	},
}
SKIN.Name = "Default"
local skins = { Default = SKIN }

------------------------------------------------------------------------ panel registry
local classes = {}
local Panel = {}
Panel.__index = Panel
Panel.ClassName = "Panel"
local world_panel
local all_panels = setmetatable({}, { __mode = "k" })
local serial = 0
local hovered, focused, pressed, pressed_button
local popups = 0
local cursor_x, cursor_y, cursor_visible = 0, 0, false

local function inherit(tbl, base)
	local b = type(base) == "table" and base or classes[base or "Panel"] or Panel
	tbl.BaseClass = b
	return setmetatable(tbl, { __index = b })
end

derma = derma or {}
vgui = vgui or {}
function vgui.Register(name, tbl, base)
	tbl = tbl or {}
	tbl.ClassName = name
	inherit(tbl, base)
	classes[name] = tbl
	return tbl
end
vgui.RegisterTable = vgui.Register
function vgui.GetControlTable(name) return classes[name] end
derma.Controls = {}
function derma.DefineControl(name, desc, tbl, base)
	local t = vgui.Register(name, tbl, base)
	derma.Controls[#derma.Controls + 1] = { ClassName = name, Description = desc or "", BaseClass = type(base) == "string" and base or "Panel" }
	return t
end
function derma.GetControlList() return derma.Controls end
function derma.GetDefaultSkin() return SKIN end
function derma.GetSkinTable() return skins end
function derma.GetNamedSkin(name) return skins[name] end
function derma.SkinList() return skins end
function derma.DefineSkin(name, desc, tbl) tbl.Name = name; tbl.Description = desc; skins[name] = setmetatable(tbl, { __index = SKIN }) end
function derma.RefreshSkins() end
function derma.SkinChangeIndex() return 0 end
function derma.Color(name, panel, default)
	local skin = panel and panel:GetSkin() or SKIN
	return skin[name] or default
end
function derma.SkinHook(kind, name, panel, w, h, ...)
	local skin = panel and panel:GetSkin() or SKIN
	local fn = skin[kind .. name]
	if fn then return fn(skin, panel, w, h, ...) end
end
function derma.SkinTexture(name, panel, default) return default end
function DermaSkin() return "Default" end
function SetDermaSkin() end

local function new_panel(class, parent, name)
	local tbl = classes[class]
	if not tbl then return nil end
	serial = serial + 1
	local p = setmetatable({
		__class = class, __serial = serial, __valid = true,
		x = 0, y = 0, w = 64, h = 24, visible = true, zpos = 0, alpha = 255,
		children = {}, mouse_input = true, keyboard_input = false, enabled = true,
		__name = name, __invalid = true, __dock = NODOCK,
		__margin = { 0, 0, 0, 0 }, __padding = { 0, 0, 0, 0 }, __cursor = "arrow",
	}, { __index = tbl })
	all_panels[p] = true
	if parent ~= nil and parent ~= false then
		p:SetParent(parent)
	elseif world_panel then
		p:SetParent(world_panel)
	end
	if p.Init then p:Init() end
	return p
end
function vgui.Create(class, parent, name) return new_panel(class, parent, name) end
function vgui.CreateFromTable(tbl, parent, name)
	serial = serial + 1
	local cls = "__anon" .. serial
	vgui.Register(cls, tbl, tbl.Base or "Panel")
	return new_panel(cls, parent, name)
end
function vgui.GetWorldPanel() return world_panel end
function vgui.GetHoveredPanel() return hovered end
function vgui.CursorVisible() return cursor_visible end
function vgui.IsHoveringWorld() return hovered == nil or hovered == world_panel end
function vgui.GetKeyboardFocus() return focused end
function vgui.FocusedHasParent(parent)
	local p = focused
	while p do if p == parent then return true end p = p.parent end
	return false
end
function vgui.GetAll()
	local out = {}
	for p in pairs(all_panels) do if p.__valid then out[#out + 1] = p end end
	return out
end

------------------------------------------------------------------------ Panel: the base
function Panel:IsValid() return self.__valid == true end
function Panel:Valid() return self.__valid == true end
function Panel:GetClassName() return self.__class end
function Panel:GetName() return self.__name or "" end
function Panel:SetName(n) self.__name = n end
function Panel:GetTable() return self end
function Panel:GetSkin() return self.__skin or (self.parent and self.parent:GetSkin()) or SKIN end
function Panel:SetSkin(name) self.__skin = skins[name] end
function Panel:Init() end
function Panel:Think() end
function Panel:Paint(w, h) end
function Panel:PaintOver(w, h) end
function Panel:PerformLayout(w, h) end
function Panel:OnRemove() end
function Panel:OnChildAdded(child) end
function Panel:OnChildRemoved(child) end
function Panel:OnCursorEntered() end
function Panel:OnCursorExited() end
function Panel:OnCursorMoved(x, y) end
function Panel:OnMousePressed(code) end
function Panel:OnMouseReleased(code) end
function Panel:OnMouseWheeled(d) end
function Panel:OnKeyCodePressed(code) end
function Panel:OnKeyCodeReleased(code) end
function Panel:OnKeyCodeTyped(code) end
function Panel:OnFocusChanged(gained) end
function Panel:OnGetFocus() end
function Panel:OnLoseFocus() end
function Panel:OnSizeChanged(w, h) end
function Panel:OnScreenSizeChanged() end
function Panel:ApplySchemeSettings() end
function Panel:AnimationThink() end

function Panel:SetParent(parent)
	if self.parent then
		local c = self.parent.children
		for i = #c, 1, -1 do if c[i] == self then table.remove(c, i) end end
		self.parent:OnChildRemoved(self)
		self.parent:InvalidateLayout()
	end
	self.parent = parent
	if parent then
		parent.children[#parent.children + 1] = self
		parent:OnChildAdded(self)
		parent:InvalidateLayout()
	end
end
function Panel:GetParent() return self.parent end
function Panel:GetChildren() return { table.unpack(self.children) } end
function Panel:HasParent(p) local q = self.parent; while q do if q == p then return true end q = q.parent end return false end
function Panel:HasChildren() return #self.children > 0 end
function Panel:Add(class) return vgui.Create(class, self) end
function Panel:Remove()
	if not self.__valid then return end
	for i = #self.children, 1, -1 do self.children[i]:Remove() end
	self:OnRemove()
	self.__valid = false
	if hovered == self then hovered = nil end
	if focused == self then focused = nil end
	if pressed == self then pressed = nil end
	if self.__popup then popups = popups - 1 end
	if self.parent then
		local c = self.parent.children
		for i = #c, 1, -1 do if c[i] == self then table.remove(c, i) end end
		self.parent:OnChildRemoved(self)
		self.parent:InvalidateLayout()
		self.parent = nil
	end
end
function Panel:Clear() for i = #self.children, 1, -1 do self.children[i]:Remove() end end
function Panel:IsMarkedForDeletion() return not self.__valid end

function Panel:SetPos(x, y) self.x, self.y = math.floor(x or 0), math.floor(y or 0) end
function Panel:GetPos() return self.x, self.y end
function Panel:GetX() return self.x end
function Panel:GetY() return self.y end
function Panel:SetX(x) self.x = math.floor(x) end
function Panel:SetY(y) self.y = math.floor(y) end
function Panel:SetSize(w, h)
	w, h = math.floor(w or self.w), math.floor(h or self.h)
	if w == self.w and h == self.h then return end
	self.w, self.h = w, h
	self:InvalidateLayout()
	self:OnSizeChanged(w, h)
end
function Panel:GetSize() return self.w, self.h end
function Panel:SetWide(w) self:SetSize(w, self.h) end
function Panel:SetTall(h) self:SetSize(self.w, h) end
function Panel:GetWide() return self.w end
function Panel:GetTall() return self.h end
function Panel:SetWidth(w) self:SetWide(w) end
function Panel:SetHeight(h) self:SetTall(h) end
function Panel:GetBounds() return self.x, self.y, self.w, self.h end
function Panel:StretchToParent(l, t, r, b)
	local pw, ph = self.parent and self.parent:GetSize()
	if not pw then return end
	self:SetPos(l or 0, t or 0)
	self:SetSize(pw - (l or 0) - (r or 0), ph - (t or 0) - (b or 0))
end
function Panel:Center() self:CenterHorizontal(); self:CenterVertical() end
function Panel:CenterHorizontal(f)
	local pw = self.parent and self.parent.w or ScrW()
	self.x = math.floor(pw * (f or 0.5) - self.w / 2)
end
function Panel:CenterVertical(f)
	local ph = self.parent and self.parent.h or ScrH()
	self.y = math.floor(ph * (f or 0.5) - self.h / 2)
end
function Panel:AlignLeft(o) self.x = o or 0 end
function Panel:AlignTop(o) self.y = o or 0 end
function Panel:AlignRight(o) self.x = (self.parent and self.parent.w or ScrW()) - self.w - (o or 0) end
function Panel:AlignBottom(o) self.y = (self.parent and self.parent.h or ScrH()) - self.h - (o or 0) end
function Panel:MoveLeftOf(p, o) self.x = p.x - self.w - (o or 0) end
function Panel:MoveRightOf(p, o) self.x = p.x + p.w + (o or 0) end
function Panel:MoveAbove(p, o) self.y = p.y - self.h - (o or 0) end
function Panel:MoveBelow(p, o) self.y = p.y + p.h + (o or 0) end
function Panel:CopyPos(p) self.x, self.y = p.x, p.y end
function Panel:CopyBounds(p) self.x, self.y = p.x, p.y; self:SetSize(p.w, p.h) end
function Panel:CopyWidth(p) self:SetWide(p.w) end
function Panel:CopyHeight(p) self:SetTall(p.h) end
function Panel:LocalToScreen(x, y)
	local p = self
	while p do x, y = x + p.x, y + p.y; p = p.parent end
	return x, y
end
function Panel:ScreenToLocal(x, y)
	local sx, sy = self:LocalToScreen(0, 0)
	return x - sx, y - sy
end
function Panel:CursorPos() return self:ScreenToLocal(cursor_x, cursor_y) end
function Panel:LocalCursorPos() return self:CursorPos() end
function Panel:IsHovered() return hovered == self end
function Panel:IsChildHovered(immediate)
	local p = hovered
	while p do
		if p == self then return true end
		if immediate then return p.parent == self end
		p = p.parent
	end
	return false
end
function Panel:IsVisible()
	local p = self
	while p do if not p.visible then return false end p = p.parent end
	return true
end
function Panel:SetVisible(v) if self.visible ~= (v == true) then self.visible = v == true; if self.parent then self.parent:InvalidateLayout() end end end
function Panel:Show() self:SetVisible(true) end
function Panel:Hide() self:SetVisible(false) end
function Panel:ToggleVisible() self:SetVisible(not self.visible) end
function Panel:SetZPos(z) self.zpos = z or 0 end
function Panel:GetZPos() return self.zpos end
function Panel:MoveToFront() if self.parent then local c = self.parent.children; for i = #c, 1, -1 do if c[i] == self then table.remove(c, i) end end; c[#c + 1] = self end end
function Panel:MoveToBack() if self.parent then local c = self.parent.children; for i = #c, 1, -1 do if c[i] == self then table.remove(c, i) end end; table.insert(c, 1, self) end end
function Panel:MoveToAfter(p) self:MoveToFront() end
function Panel:MoveToBefore(p) self:MoveToBack() end
function Panel:SetAlpha(a) self.alpha = a or 255 end
function Panel:GetAlpha() return self.alpha end
function Panel:SetMouseInputEnabled(b) self.mouse_input = b == true end
function Panel:IsMouseInputEnabled() return self.mouse_input end
function Panel:SetKeyboardInputEnabled(b) self.keyboard_input = b == true end
function Panel:IsKeyboardInputEnabled() return self.keyboard_input end
function Panel:SetKeyBoardInputEnabled(b) self:SetKeyboardInputEnabled(b) end
function Panel:SetEnabled(b) self.enabled = b ~= false end
function Panel:IsEnabled() return self.enabled end
function Panel:SetDisabled(b) self.enabled = not b end
function Panel:GetDisabled() return not self.enabled end
function Panel:SetCursor(c) self.__cursor = c end
function Panel:GetCursor() return self.__cursor end
function Panel:SetTooltip(t) self.__tooltip = t end
function Panel:SetTooltipPanel(p) self.__tooltip_panel = p end
function Panel:SetTooltipDelay(d) end
function Panel:SetPaintBackground(b) self.__paint_bg = b ~= false end
function Panel:GetPaintBackground() return self.__paint_bg ~= false end
function Panel:SetPaintBackgroundEnabled(b) self:SetPaintBackground(b) end
function Panel:SetPaintBorderEnabled(b) end
function Panel:SetPaintedManually(b) self.__manual = b == true end
function Panel:SetDrawOnTop(b) end
function Panel:NoClipping(b) self.__noclip = b == true end
function Panel:ParentToHUD() end
function Panel:SetWorldClicker(b) end
function Panel:IsWorldClicker() return false end
function Panel:SetPopupStayAtBack(b) end
function Panel:SetRenderInScreenshots(b) end
function Panel:SetTerm() end
function Panel:SetAutoDelete(b) end
function Panel:GetAutoDelete() return true end
function Panel:SetSelectable(b) end
function Panel:IsSelectable() return false end
function Panel:SetSelected(b) self.__selected = b == true end
function Panel:IsSelected() return self.__selected == true end
function Panel:ToggleSelection() self.__selected = not self.__selected end
function Panel:SetCommand(c) self.__command = c end
function Panel:SetConVar(c) self.__convar = c end
function Panel:ConVarChanged(v) if self.__convar then RunConsoleCommand(self.__convar, tostring(v)) end end
function Panel:ConVarStringThink() end
function Panel:ConVarNumberThink() end
function Panel:MakePopup()
	if not self.__popup then self.__popup = true; popups = popups + 1 end
	self:MoveToFront()
	self:SetMouseInputEnabled(true)
	self:SetKeyboardInputEnabled(true)
	self:RequestFocus()
end
function Panel:IsPopup() return self.__popup == true end
function Panel:MouseCapture(b) if b then pressed = self elseif pressed == self then pressed = nil end end
function Panel:HasFocus() return focused == self end
function Panel:HasHierarchicalFocus() return vgui.FocusedHasParent(self) end
function Panel:RequestFocus()
	if focused == self then return end
	local old = focused
	focused = self
	if old and old.__valid then old:OnFocusChanged(false); old:OnLoseFocus() end
	self:OnFocusChanged(true)
	self:OnGetFocus()
end
function Panel:KillFocus() if focused == self then focused = nil; self:OnFocusChanged(false); self:OnLoseFocus() end end
function Panel:InvalidateLayout(now)
	self.__invalid = true
	if now then self:__layout() end
end
function Panel:InvalidateParent(now) if self.parent then self.parent:InvalidateLayout(now) end end
function Panel:InvalidateChildren(recurse)
	for _, c in ipairs(self.children) do c.__invalid = true; if recurse then c:InvalidateChildren(true) end end
end
function Panel:Dock(d) self.__dock = d or NODOCK; self:InvalidateParent() end
function Panel:GetDock() return self.__dock end
function Panel:DockMargin(l, t, r, b) self.__margin = { l or 0, t or 0, r or 0, b or 0 }; self:InvalidateParent() end
function Panel:GetDockMargin() return table.unpack(self.__margin) end
function Panel:DockPadding(l, t, r, b) self.__padding = { l or 0, t or 0, r or 0, b or 0 }; self:InvalidateLayout() end
function Panel:GetDockPadding() return table.unpack(self.__padding) end
-- docking as the game lays it out: children in z order (stable), LEFT / RIGHT / TOP / BOTTOM
-- take their edge inside the padding, FILL takes what is left, margins around each
function Panel:__layout()
	if not self.__invalid then return end
	self.__invalid = false
	local pl, pt, pr, pb = table.unpack(self.__padding)
	local x, y, r, b = pl, pt, self.w - pr, self.h - pb
	local order = self:__ordered()
	for _, c in ipairs(order) do
		if c.visible and c.__dock ~= NODOCK and c.__dock ~= FILL then
			local ml, mt, mr, mb = table.unpack(c.__margin)
			if c.__dock == TOP then c:SetPos(x + ml, y + mt); c:SetSize(r - x - ml - mr, c.h); y = y + c.h + mt + mb
			elseif c.__dock == BOTTOM then c:SetPos(x + ml, b - mb - c.h); c:SetSize(r - x - ml - mr, c.h); b = b - c.h - mt - mb
			elseif c.__dock == LEFT then c:SetPos(x + ml, y + mt); c:SetSize(c.w, b - y - mt - mb); x = x + c.w + ml + mr
			elseif c.__dock == RIGHT then c:SetPos(r - mr - c.w, y + mt); c:SetSize(c.w, b - y - mt - mb); r = r - c.w - ml - mr end
		end
	end
	for _, c in ipairs(order) do
		if c.visible and c.__dock == FILL then
			local ml, mt, mr, mb = table.unpack(c.__margin)
			c:SetPos(x + ml, y + mt)
			c:SetSize(r - x - ml - mr, b - y - mt - mb)
		end
	end
	self:PerformLayout(self.w, self.h)
end
function Panel:__ordered()
	local order = { table.unpack(self.children) }
	for i, c in ipairs(order) do c.__ord = i end
	table.sort(order, function(a, b) if a.zpos ~= b.zpos then return a.zpos < b.zpos end return a.__ord < b.__ord end)
	return order
end
function Panel:ChildrenSize()
	local w, h = 0, 0
	for _, c in ipairs(self.children) do if c.visible then w = math.max(w, c.x + c.w); h = math.max(h, c.y + c.h) end end
	return w, h
end
function Panel:SizeToChildren(sw, sh)
	local w, h = self:ChildrenSize()
	local pl, pt, pr, pb = table.unpack(self.__padding)
	self:SetSize(sw and (w + pr) or self.w, sh and (h + pb) or self.h)
end
function Panel:SizeToContents() end
function Panel:SizeToContentsX(add) end
function Panel:SizeToContentsY(add) end

-- text: the base keeps the label state every text control shares
function Panel:SetText(t) self.__text = tostring(t == nil and "" or t); self:InvalidateLayout() end
function Panel:GetText() return self.__text or "" end
function Panel:GetValue() return self:GetText() end
function Panel:SetValue(v) self:SetText(v) end
function Panel:SetFont(f) self.__font = f end
function Panel:GetFont() return self.__font or "DermaDefault" end
function Panel:SetFontInternal(f) self.__font = f end
function Panel:SetTextColor(c) self.__text_color = c end
function Panel:GetTextColor() return self.__text_color end
function Panel:SetColor(c) self.__text_color = c end
function Panel:GetColor() return self.__text_color end
function Panel:SetFGColor(r, g, b, a) self.__text_color = type(r) == "table" and r or Color(r, g, b, a) end
function Panel:SetBGColor(r, g, b, a) self.__bg_color = type(r) == "table" and r or Color(r, g, b, a) end
function Panel:SetBackgroundColor(c) self.__bg_color = c end
function Panel:GetBackgroundColor() return self.__bg_color end
function Panel:SetContentAlignment(a) self.__align = a or 4 end
function Panel:GetContentAlignment() return self.__align or 4 end
function Panel:SetTextInset(x, y) self.__inset = { x or 0, y or 0 } end
function Panel:GetTextInset() return table.unpack(self.__inset or { 0, 0 }) end
function Panel:SetExpensiveShadow(d, c) self.__shadow = { d or 1, c } end
function Panel:SetWrap(b) self.__wrap = b == true end
function Panel:GetWrap() return self.__wrap == true end
function Panel:SetAutoStretchVertical(b) self.__auto_v = b == true end
function Panel:GetTextSize()
	surface.SetFont(self:GetFont())
	return surface.GetTextSize(self:GetText())
end
function Panel:GetContentSize() return self:GetTextSize() end
-- the game draws a label's text by its content alignment (numpad: 1 bottom-left .. 9 top-right)
function Panel:__draw_text(w, h, color)
	local text = self:GetText()
	if text == "" then return end
	surface.SetFont(self:GetFont())
	local tw, th = surface.GetTextSize(text)
	local a = self.__align or 4
	local ix, iy = table.unpack(self.__inset or { 0, 0 })
	local x = ix
	if a == 2 or a == 5 or a == 8 then x = (w - tw) / 2 elseif a == 3 or a == 6 or a == 9 then x = w - tw - ix end
	local y = iy
	if a >= 4 and a <= 6 then y = (h - th) / 2 elseif a <= 3 then y = h - th - iy end
	if self.__shadow then
		surface.SetTextColor(self.__shadow[2] or Color(0, 0, 0, 255))
		surface.SetTextPos(math.floor(x) + self.__shadow[1], math.floor(y) + self.__shadow[1])
		surface.DrawText(text)
	end
	surface.SetTextColor(color or self.__text_color or Color(255, 255, 255))
	surface.SetTextPos(math.floor(x), math.floor(y))
	surface.DrawText(text)
end

-- animations (Derma_Anim / MoveTo / SizeTo / AlphaTo / ColorTo), advanced in the frame
function Panel:NewAnimation(length, delay, ease, callback)
	local anim = { StartTime = SysTime() + (delay or 0), EndTime = SysTime() + (delay or 0) + (length or 0), Ease = ease or -1, OnEnd = callback }
	self.__anims = self.__anims or {}
	self.__anims[#self.__anims + 1] = anim
	return anim
end
local function run_anims(p)
	if not p.__anims then return end
	local now = SysTime()
	for i = #p.__anims, 1, -1 do
		local a = p.__anims[i]
		if now >= a.StartTime then
			local f = a.EndTime > a.StartTime and math.min(1, (now - a.StartTime) / (a.EndTime - a.StartTime)) or 1
			if a.Ease and a.Ease > 0 then f = f ^ a.Ease elseif a.Ease and a.Ease < 0 and a.Ease ~= -1 then f = 1 - (1 - f) ^ -a.Ease end
			if a.Think then a:Think(p, f) end
			if f >= 1 then
				table.remove(p.__anims, i)
				if a.OnEnd then a.OnEnd(a, p) end
			end
		end
	end
end
function Panel:MoveTo(x, y, len, delay, ease, cb)
	local a = self:NewAnimation(len, delay, ease, cb)
	a.StartPos, a.Pos = nil, { x, y }
	a.Think = function(an, pnl, f) if not an.StartPos then an.StartPos = { pnl.x, pnl.y } end pnl:SetPos(an.StartPos[1] + (an.Pos[1] - an.StartPos[1]) * f, an.StartPos[2] + (an.Pos[2] - an.StartPos[2]) * f) end
	return a
end
function Panel:SizeTo(w, h, len, delay, ease, cb)
	local a = self:NewAnimation(len, delay, ease, cb)
	a.Size = { w, h }
	a.Think = function(an, pnl, f) if not an.StartSize then an.StartSize = { pnl.w, pnl.h } end pnl:SetSize(an.StartSize[1] + ((an.Size[1] < 0 and pnl.w or an.Size[1]) - an.StartSize[1]) * f, an.StartSize[2] + ((an.Size[2] < 0 and pnl.h or an.Size[2]) - an.StartSize[2]) * f) end
	return a
end
function Panel:AlphaTo(alpha, len, delay, cb)
	local a = self:NewAnimation(len, delay, nil, cb)
	a.Alpha = alpha
	a.Think = function(an, pnl, f) if not an.StartAlpha then an.StartAlpha = pnl.alpha end pnl:SetAlpha(an.StartAlpha + (an.Alpha - an.StartAlpha) * f) end
	return a
end
function Panel:ColorTo(col, len, delay, cb)
	local a = self:NewAnimation(len, delay, nil, cb)
	a.Color = col
	a.Think = function(an, pnl, f)
		if not an.StartColor then an.StartColor = pnl.__bg_color or Color(255, 255, 255) end
		local s, e = an.StartColor, an.Color
		pnl:SetBackgroundColor(Color(s.r + (e.r - s.r) * f, s.g + (e.g - s.g) * f, s.b + (e.b - s.b) * f, (s.a or 255) + ((e.a or 255) - (s.a or 255)) * f))
	end
	return a
end
function Panel:Stop() self.__anims = nil end
function Panel:LerpPositions(speed, ease) end
function Panel:SetPaintFunction(name, fn) self[name] = fn end
function Panel:Queue() end
function Panel:Droppable() end
function Panel:Receiver() end
function Panel:DragMousePress() end
function Panel:DragMouseRelease() return false end
function Panel:IsDragging() return false end
function Panel:DragHover() end
function Panel:DragHoverEnd() end
function Panel:SetDropTarget() end
function Panel:DrawFilledRect() surface.DrawRect(0, 0, self.w, self.h) end
function Panel:DrawOutlinedRect() surface.DrawOutlinedRect(0, 0, self.w, self.h) end
function Panel:DrawTexturedRect() surface.DrawTexturedRect(0, 0, self.w, self.h) end
function Panel:DrawDragHover() end
function Panel:DrawSelections() end
function Panel:SetAchievement() end
function Panel:SetPlayer() end
function Panel:SetModel() end
function Panel:SetSteamID() end
function Panel:Distance(p) local ax, ay = self:LocalToScreen(self.w / 2, self.h / 2); local bx, by = p:LocalToScreen(p.w / 2, p.h / 2); return math.sqrt((ax - bx) ^ 2 + (ay - by) ^ 2) end
function Panel:DistanceFrom(x, y) local ax, ay = self:LocalToScreen(self.w / 2, self.h / 2); return math.sqrt((ax - x) ^ 2 + (ay - y) ^ 2) end
function Panel:IsOurChild(p) return p and p:HasParent(self) end
function Panel:GetChild(i) return self.children[(i or 0) + 1] end
function Panel:ChildCount() return #self.children end
function Panel:Find(name) for p in pairs(all_panels) do if p.__valid and p.__name == name then return p end end end
function Panel:GetChildPosition(c) for i, k in ipairs(self.children) do if k == c then return i - 1 end end return -1 end
function Panel:GetClosestChild(x, y)
	local best, bd
	for _, c in ipairs(self.children) do local d = c:DistanceFrom(x, y); if not bd or d < bd then best, bd = c, d end end
	return best
end
function Panel:SetAnimationEnabled(b) end
function Panel:GetAnimationEnabled() return true end
function Panel:AppendText(t) self:SetText(self:GetText() .. tostring(t)) end
function Panel:SetTooltipPanelOverride() end
function Panel:SetPaintBackgroundEnabled(b) self:SetPaintBackground(b) end
function Panel:ApplyGWEN() end
function Panel:GetDockMarginLeft() return self.__margin[1] end
function Panel:SetConsoleCommand() end
function Panel:SetSteamID64() end
function Panel:Exec(cmd) RunConsoleCommand(cmd) end
function Panel:FocusNext() end
function Panel:FocusPrevious() end
function Panel:SetFocusTopLevel(b) end
function Panel:CopySelected() end
function Panel:Paste() end
function Panel:Cut() end
function Panel:GotoTextStart() end
function Panel:GotoTextEnd() end
function Panel:SetCaretPos(i) self.__caret = i end
function Panel:GetCaretPos() return self.__caret or #self:GetText() end
function Panel:SetUnderlineFont(f) end
function Panel:SetMultiline(b) self.__multiline = b == true end
function Panel:IsMultiline() return self.__multiline == true end
function Panel:SetMaximumCharCount(n) self.__max_chars = n end
function Panel:SetAllowNonAsciiCharacters(b) end
function Panel:SetVerticalScrollbarEnabled(b) end
function Panel:SetHistoryEnabled(b) end
function Panel:SetEditable(b) self.__editable = b ~= false end
function Panel:IsEditing() return focused == self end
function Panel:SetDrawLanguageID() end
function Panel:SetDrawLanguageIDAtLeft() end
function Panel:SetDrawBackground(b) self:SetPaintBackground(b) end
function Panel:GetDrawBackground() return self:GetPaintBackground() end
function Panel:SetDrawBorder(b) end
function Panel:SetOpenLinksExternally() end
function Panel:SetImage(m) self.__image = type(m) == "string" and Material(m) or m end
function Panel:GetImage() return self.__image and self.__image:GetName() or "" end
function Panel:SetMaterial(m) self.__image = m end
function Panel:GetMaterial() return self.__image end
function Panel:SetSpawnIcon() end
function Panel:SetTextSelectionColors() end
function Panel:SetScheme() end
function Panel:SetTabPosition() end
function Panel:ValidateLayout() self:__layout() end
function Panel:SetFGColorEx() end
function Panel:SetBGColorEx() end
function Panel:SetShadow() end
function Panel:SetToFullScreen() self:SetPos(0, 0); self:SetSize(ScrW(), ScrH()) end
function Panel:SetTooltipDelay() end
function Panel:RebuildSpawnIcon() end
function Panel:RebuildSpawnIconEx() end
function Panel:GetTooltip() return self.__tooltip end
function Panel:GetTooltipPanel() return self.__tooltip_panel end
function Panel:GetPanelAt() end
classes.Panel = Panel

------------------------------------------------------------------------ the frame: layout, hover, paint
local function hit_test(p, sx, sy)
	if not p.visible or not p.__valid then return nil end
	if sx < 0 or sy < 0 or sx >= p.w or sy >= p.h then return nil end
	local order = p:__ordered()
	for i = #order, 1, -1 do
		local c = order[i]
		local hit = hit_test(c, sx - c.x, sy - c.y)
		if hit then return hit end
	end
	if p.mouse_input then return p end
	return nil
end
local function paint(p, sx, sy)
	if not p.visible or not p.__valid or p.__manual then return end
	p:__layout()
	surface.__origin(sx, sy)
	surface.SetAlphaMultiplier(p.alpha / 255)
	if p.Paint then p:Paint(p.w, p.h) end
	for _, c in ipairs(p:__ordered()) do paint(c, sx + c.x, sy + c.y) end
	surface.__origin(sx, sy)
	surface.SetAlphaMultiplier(p.alpha / 255)
	if p.PaintOver then p:PaintOver(p.w, p.h) end
end
local function think(p)
	if not p.__valid then return end
	run_anims(p)
	p:Think()
	p:AnimationThink()
	for i = #p.children, 1, -1 do local c = p.children[i]; if c then think(c) end end
end
local function update_hover()
	local target = cursor_visible and hit_test(world_panel, cursor_x, cursor_y) or nil
	if target == world_panel then target = nil end
	if target ~= hovered then
		if hovered and hovered.__valid then hovered:OnCursorExited() end
		hovered = target
		if hovered then hovered:OnCursorEntered() end
	end
	if hovered then hovered:OnCursorMoved(hovered:CursorPos()) end
end

world_panel = new_panel("Panel", false, "GModBase")
world_panel.mouse_input = true
world_panel:SetSize(ScrW(), ScrH())
world_panel.Paint = function() end

if events then
	events.on("render.hud", function(p)
		cursor_visible = p.cursor == true
		if p.mx then cursor_x, cursor_y = p.mx, p.my end
		if world_panel.w ~= (p.w or world_panel.w) or world_panel.h ~= (p.h or world_panel.h) then
			world_panel:SetSize(p.w or world_panel.w, p.h or world_panel.h)
			for q in pairs(all_panels) do if q.__valid then q:OnScreenSizeChanged() end end
		end
		think(world_panel)
		update_hover()
	end)
end
-- painted from the HUD frame (client.luau) after `PostDrawHUD`: the game's vgui layer sits
-- above the HUD, `PostRenderVGUI` after it
function __derma_paint()
	hook.Run("PreRenderVGUI")
	for _, c in ipairs(world_panel:__ordered()) do paint(c, c.x, c.y) end
	surface.__origin(0, 0)
	surface.SetAlphaMultiplier(1)
	hook.Run("PostRenderVGUI")
end

------------------------------------------------------------------------ input
-- the client's physical key names -> the game's KEY_ / MOUSE_ codes and typed characters
local KEYNAMES = { Enter = "KEY_ENTER", NumpadEnter = "KEY_PAD_ENTER", Space = "KEY_SPACE", Backspace = "KEY_BACKSPACE", Tab = "KEY_TAB", Escape = "KEY_ESCAPE", Delete = "KEY_DELETE", Insert = "KEY_INSERT", Home = "KEY_HOME", End = "KEY_END", PageUp = "KEY_PAGEUP", PageDown = "KEY_PAGEDOWN", ArrowUp = "KEY_UP", ArrowDown = "KEY_DOWN", ArrowLeft = "KEY_LEFT", ArrowRight = "KEY_RIGHT", ShiftLeft = "KEY_LSHIFT", ShiftRight = "KEY_RSHIFT", ControlLeft = "KEY_LCONTROL", ControlRight = "KEY_RCONTROL", AltLeft = "KEY_LALT", AltRight = "KEY_RALT", CapsLock = "KEY_CAPSLOCK", Minus = "KEY_MINUS", Equal = "KEY_EQUAL", BracketLeft = "KEY_LBRACKET", BracketRight = "KEY_RBRACKET", Semicolon = "KEY_SEMICOLON", Quote = "KEY_APOSTROPHE", Backquote = "KEY_BACKQUOTE", Comma = "KEY_COMMA", Period = "KEY_PERIOD", Slash = "KEY_SLASH", Backslash = "KEY_BACKSLASH", MouseLeft = "MOUSE_LEFT", MouseRight = "MOUSE_RIGHT", MouseMiddle = "MOUSE_MIDDLE", MouseBack = "MOUSE_4", MouseForward = "MOUSE_5", WheelUp = "MOUSE_WHEEL_UP", WheelDown = "MOUSE_WHEEL_DOWN" }
local SHIFTED = { ["1"] = "!", ["2"] = "@", ["3"] = "#", ["4"] = "$", ["5"] = "%", ["6"] = "^", ["7"] = "&", ["8"] = "*", ["9"] = "(", ["0"] = ")", ["-"] = "_", ["="] = "+", ["["] = "{", ["]"] = "}", [";"] = ":", ["'"] = '"', ["`"] = "~", [","] = "<", ["."] = ">", ["/"] = "?", ["\\"] = "|" }
local PUNCT = { Minus = "-", Equal = "=", BracketLeft = "[", BracketRight = "]", Semicolon = ";", Quote = "'", Backquote = "`", Comma = ",", Period = ".", Slash = "/", Backslash = "\\", Space = " " }
local shift_down = false
local function key_code(name)
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
local function key_char(name)
	local letter = name:match("^Key(%a)$")
	if letter then return shift_down and letter:upper() or letter:lower() end
	local digit = name:match("^Digit(%d)$")
	if digit then return shift_down and SHIFTED[digit] or digit end
	local pad = name:match("^Numpad(%d)$")
	if pad then return pad end
	local p = PUNCT[name]
	if p then return shift_down and SHIFTED[p] or p end
end
function __derma_key(name, down)
	if name == "ShiftLeft" or name == "ShiftRight" then shift_down = down end
	local code = key_code(name)
	if not code then return false end
	if code >= MOUSE_LEFT and code <= MOUSE_5 then
		if not cursor_visible then return false end
		update_hover()
		if down then
			local target = hovered
			if target then
				pressed, pressed_button = target, code
				if not target.__focus_passive then target:RequestFocus() end
				target:OnMousePressed(code)
			else
				if focused and focused.__valid then focused:KillFocus() end
			end
			-- a click outside any open DMenu closes them
			if not (target and (target:GetClassName() == "DMenu" or target:HasParent(nil))) then end
			for p in pairs(all_panels) do if p.__valid and p.__is_menu and not (target and (target == p or target:HasParent(p))) then p:Remove() end end
			return target ~= nil
		else
			local target = pressed
			pressed, pressed_button = nil, nil
			if target and target.__valid then target:OnMouseReleased(code); return true end
			return hovered ~= nil
		end
	end
	if code == MOUSE_WHEEL_UP or code == MOUSE_WHEEL_DOWN then
		if down and hovered then
			local p = hovered
			while p do if p:OnMouseWheeled(code == MOUSE_WHEEL_UP and 1 or -1) then return true end p = p.parent end
		end
		return false
	end
	if focused and focused.__valid and focused.keyboard_input then
		if down then
			if focused:OnKeyCodePressed(code) then return true end
			local ch = key_char(name)
			if ch and focused.OnKeyChar then focused:OnKeyChar(ch) end
			if focused:OnKeyCodeTyped(code) then return true end
		else
			focused:OnKeyCodeReleased(code)
		end
		return true
	end
	return false
end

------------------------------------------------------------------------ controls
local EditablePanel = vgui.Register("EditablePanel", {}, "Panel")

local DPanel = derma.DefineControl("DPanel", "A simple rectangular box", {}, "Panel")
function DPanel:Init() self.__paint_bg = true end
function DPanel:Paint(w, h)
	if not self.__paint_bg then return end
	local c = self.__bg_color or (self.enabled and Color(255, 255, 255, 255) or Color(255, 255, 255, 255))
	surface.SetDrawColor(c)
	surface.DrawRect(0, 0, w, h)
	-- Panels.Normal: a soft 1 px border of the skin's dark colour
	surface.SetDrawColor(SKIN.bg_color_dark)
	surface.DrawOutlinedRect(0, 0, w, h)
end
function DPanel:SetIsMenu(b) self.__is_menu = b == true end
function DPanel:GetIsMenu() return self.__is_menu == true end

local DLabel = derma.DefineControl("DLabel", "A simple label", {}, "Panel")
function DLabel:Init() self.mouse_input = false; self.__text_color = nil; self.__auto_v = false end
function DLabel:SetAutoStretchVertical(b) self.__auto_v = b == true end
function DLabel:GetAutoStretchVertical() return self.__auto_v end
function DLabel:SizeToContents() local w, h = self:GetTextSize(); self:SetSize(w, h) end
function DLabel:SizeToContentsX(add) local w = self:GetTextSize(); self:SetWide(w + (add or 0)) end
function DLabel:SizeToContentsY(add) local _, h = self:GetTextSize(); self:SetTall(h + (add or 0)) end
function DLabel:SetDark(b) self.__dark = b; if b then self.__text_color = SKIN.Colours.Label.Dark end end
function DLabel:GetDark() return self.__dark == true end
function DLabel:SetBright(b) self.__bright = b; if b then self.__text_color = SKIN.Colours.Label.Bright end end
function DLabel:GetBright() return self.__bright == true end
function DLabel:SetHighlight(b) self.__hl = b; if b then self.__text_color = SKIN.Colours.Label.Highlight end end
function DLabel:GetHighlight() return self.__hl == true end
function DLabel:SetIsToggle(b) self.__toggle = b == true end
function DLabel:GetIsToggle() return self.__toggle == true end
function DLabel:SetToggle(b) self.__toggled = b == true end
function DLabel:GetToggle() return self.__toggled == true end
function DLabel:Toggle() self.__toggled = not self.__toggled; self:OnToggled(self.__toggled) end
function DLabel:OnToggled(b) end
function DLabel:DoClick() end
function DLabel:DoRightClick() end
function DLabel:DoMiddleClick() end
function DLabel:DoDoubleClick() end
function DLabel:DoClickInternal() end
function DLabel:DoDoubleClickInternal() end
function DLabel:OnDepressed() end
function DLabel:OnReleased() end
function DLabel:IsDown() return self.__down == true end
function DLabel:SetDoubleClickingEnabled(b) end
function DLabel:UpdateFGColor() end
function DLabel:UpdateColours() end
function DLabel:SetDisabled(b) self.enabled = not b end
function DLabel:OnMousePressed(code)
	if not self.enabled then return end
	self.__down = true
	self:MouseCapture(true)
	self:OnDepressed()
end
function DLabel:OnMouseReleased(code)
	if not self.__down then return end
	self.__down = false
	self:MouseCapture(false)
	self:OnReleased()
	if self:IsHovered() and self.enabled then
		if code == MOUSE_LEFT then
			if self.__toggle then self:Toggle() end
			self:DoClickInternal()
			self:DoClick()
		elseif code == MOUSE_RIGHT then self:DoRightClick()
		elseif code == MOUSE_MIDDLE then self:DoMiddleClick() end
	end
end
function DLabel:Paint(w, h)
	self:__draw_text(w, h, self.__text_color or (self.__dark and SKIN.Colours.Label.Dark) or SKIN.Colours.Label.Default)
end
function DLabel:PerformLayout(w, h)
	if self.__auto_v then local _, th = self:GetTextSize(); if th ~= h then self:SetTall(th) end end
end

local DButton = derma.DefineControl("DButton", "A standard Button", {}, "DLabel")
function DButton:Init()
	self.mouse_input = true
	self.__align = 5
	self.__text_color = nil
	self:SetText("Button")
	self.__paint_bg = true
end
function DButton:SetConsoleCommand(cmd, args) self.DoClick = function() RunConsoleCommand(cmd, args) end end
function DButton:SetDrawBorder(b) end
function DButton:SetImage(img) self.__icon = img end
function DButton:SetIcon(img) self.__icon = img end
function DButton:GetImage() return self.__icon end
function DButton:SizeToContents() local w, h = self:GetTextSize(); self:SetSize(w + 8, h + 4) end
function DButton:Paint(w, h)
	if self.__paint_bg then
		-- Button / Hovered / Down / Dead pieces: flat fills of the skin's control colours
		local c
		if not self.enabled then c = Color(148, 148, 148)
		elseif self.__down then c = SKIN.control_color_active
		elseif self:IsHovered() then c = SKIN.control_color_highlight
		else c = SKIN.control_color end
		surface.SetDrawColor(c)
		surface.DrawRect(0, 0, w, h)
		surface.SetDrawColor(SKIN.colButtonBorder)
		surface.DrawOutlinedRect(0, 0, w, h)
	end
	local tc = self.__text_color
	if not tc then
		if not self.enabled then tc = SKIN.Colours.Button.Disabled
		elseif self.__down then tc = SKIN.Colours.Button.Down
		elseif self:IsHovered() then tc = SKIN.Colours.Button.Hover
		else tc = SKIN.Colours.Button.Normal end
	end
	self:__draw_text(w, h, tc)
end

local DFrame = derma.DefineControl("DFrame", "A simple window", {}, "EditablePanel")
function DFrame:Init()
	self.__title = "Window"
	self.__draggable, self.__sizable, self.__show_close, self.__delete_on_close = true, false, true, true
	self.__screenlock, self.__bg_blur, self.__paint_shadow = false, false, true
	self.__min_w, self.__min_h = 50, 50
	self:SetSize(320, 240)
	self.keyboard_input = true
	self.btnClose = vgui.Create("DButton", self)
	self.btnClose:SetText("r")
	self.btnClose:SetFont("Marlett")
	self.btnClose:SetSize(31, 24)
	self.btnClose.__paint_bg = false
	self.btnClose.DoClick = function() self:Close() end
	self.lblTitle = vgui.Create("DLabel", self)
	self.lblTitle:SetText(self.__title)
	self.lblTitle.__text_color = SKIN.Colours.Window.TitleActive
	self.__created = SysTime()
end
function DFrame:SetTitle(t) self.__title = tostring(t or ""); if self.lblTitle then self.lblTitle:SetText(self.__title) end end
function DFrame:GetTitle() return self.__title end
function DFrame:SetDraggable(b) self.__draggable = b == true end
function DFrame:GetDraggable() return self.__draggable end
function DFrame:SetSizable(b) self.__sizable = b == true end
function DFrame:GetSizable() return self.__sizable end
function DFrame:ShowCloseButton(b) self.__show_close = b ~= false; if self.btnClose then self.btnClose:SetVisible(self.__show_close) end end
function DFrame:SetDeleteOnClose(b) self.__delete_on_close = b ~= false end
function DFrame:GetDeleteOnClose() return self.__delete_on_close end
function DFrame:SetScreenLock(b) self.__screenlock = b == true end
function DFrame:GetScreenLock() return self.__screenlock end
function DFrame:SetBackgroundBlur(b) self.__bg_blur = b == true end
function DFrame:GetBackgroundBlur() return self.__bg_blur end
function DFrame:SetPaintShadow(b) self.__paint_shadow = b ~= false end
function DFrame:GetPaintShadow() return self.__paint_shadow end
function DFrame:SetMinWidth(w) self.__min_w = w end
function DFrame:SetMinHeight(h) self.__min_h = h end
function DFrame:GetMinWidth() return self.__min_w end
function DFrame:GetMinHeight() return self.__min_h end
function DFrame:SetIcon(path) self.__icon = path end
function DFrame:IsActive() return focused == self or vgui.FocusedHasParent(self) end
function DFrame:OnClose() end
function DFrame:Close()
	self:SetVisible(false)
	if self.__delete_on_close then self:Remove() end
	self:OnClose()
end
function DFrame:OnKeyCodePressed(code)
	if code == KEY_ESCAPE and self.__valid then self:Close(); return true end
end
function DFrame:OnMousePressed(code)
	local lx, ly = self:CursorPos()
	if self.__sizable and lx > self.w - 20 and ly > self.h - 20 then
		self.__sizing = { lx - self.w, ly - self.h }
		self:MouseCapture(true)
		return
	end
	if self.__draggable and ly < 24 then
		self.__dragging = { lx, ly }
		self:MouseCapture(true)
	end
end
function DFrame:OnMouseReleased() self.__dragging, self.__sizing = nil, nil; self:MouseCapture(false) end
function DFrame:Think()
	if self.__dragging then
		local x, y = cursor_x - self.__dragging[1], cursor_y - self.__dragging[2]
		if self.parent then x, y = self.parent:ScreenToLocal(x, y) end
		if self.__screenlock then x = math.max(0, math.min(x, ScrW() - self.w)); y = math.max(0, math.min(y, ScrH() - self.h)) end
		self:SetPos(x, y)
	end
	if self.__sizing then
		local lx, ly = self:CursorPos()
		self:SetSize(math.max(self.__min_w, lx - self.__sizing[1]), math.max(self.__min_h, ly - self.__sizing[2]))
	end
end
function DFrame:PerformLayout(w, h)
	self.btnClose:SetPos(w - 31 - 4, 0)
	self.lblTitle:SetPos(8, 2)
	self.lblTitle:SetSize(w - 25 - 16, 20)
end
function DFrame:Paint(w, h)
	-- Window.Normal: the body in bg_color_dark with a 24 px title bar of bg_color
	surface.SetDrawColor(SKIN.bg_color_dark)
	surface.DrawRect(0, 0, w, h)
	surface.SetDrawColor(SKIN.bg_color)
	surface.DrawRect(0, 0, w, 24)
	surface.SetDrawColor(Color(0, 0, 0, 200))
	surface.DrawOutlinedRect(0, 0, w, h)
end

local DCheckBox = derma.DefineControl("DCheckBox", "Checkbox", {}, "DButton")
function DCheckBox:Init() self:SetSize(15, 15); self.__checked = false; self.__text = "" end
function DCheckBox:SetChecked(b) self.__checked = b == true end
function DCheckBox:GetChecked() return self.__checked end
function DCheckBox:SetValue(v) local b = v == true or v == 1 or v == "1"; if b ~= self.__checked then self.__checked = b; self:OnChange(b); self:ConVarChanged(b and 1 or 0) end end
function DCheckBox:Toggle() self:SetValue(not self.__checked) end
function DCheckBox:OnChange(b) end
function DCheckBox:DoClick() self:Toggle() end
function DCheckBox:IsEditing() return self.__down end
function DCheckBox:Paint(w, h)
	surface.SetDrawColor(self.enabled and Color(255, 255, 255) or Color(200, 200, 200))
	surface.DrawRect(0, 0, w, h)
	surface.SetDrawColor(SKIN.colButtonBorder)
	surface.DrawOutlinedRect(0, 0, w, h)
	if self.__checked then
		surface.SetDrawColor(Color(20, 20, 20))
		surface.DrawRect(3, 3, w - 6, h - 6)
	end
end

local DCheckBoxLabel = derma.DefineControl("DCheckBoxLabel", "Checkbox with a label", {}, "DPanel")
function DCheckBoxLabel:Init()
	self.__paint_bg = false
	self.Button = vgui.Create("DCheckBox", self)
	self.Button.OnChange = function(_, b) self:OnChange(b) end
	self.Label = vgui.Create("DLabel", self)
	self.Label:SetText("")
	self.__indent = 0
	self:SetTall(15)
end
function DCheckBoxLabel:OnChange(b) end
function DCheckBoxLabel:SetText(t) self.Label:SetText(t) end
function DCheckBoxLabel:GetText() return self.Label:GetText() end
function DCheckBoxLabel:SetChecked(b) self.Button:SetChecked(b) end
function DCheckBoxLabel:GetChecked() return self.Button:GetChecked() end
function DCheckBoxLabel:SetValue(v) self.Button:SetValue(v) end
function DCheckBoxLabel:Toggle() self.Button:Toggle() end
function DCheckBoxLabel:SetConVar(c) self.Button:SetConVar(c) end
function DCheckBoxLabel:SetTextColor(c) self.Label:SetTextColor(c) end
function DCheckBoxLabel:SetFont(f) self.Label:SetFont(f) end
function DCheckBoxLabel:SetDark(b) self.Label:SetDark(b) end
function DCheckBoxLabel:SetBright(b) self.Label:SetBright(b) end
function DCheckBoxLabel:SetIndent(i) self.__indent = i or 0 end
function DCheckBoxLabel:GetIndent() return self.__indent end
function DCheckBoxLabel:SizeToContents() local w, h = self.Label:GetTextSize(); self:SetSize(self.__indent + 15 + 9 + w, math.max(15, h)) end
function DCheckBoxLabel:PerformLayout(w, h)
	self.Button:SetPos(self.__indent, math.floor(h / 2 - 15 / 2))
	self.Label:SetPos(self.__indent + 15 + 9, 0)
	self.Label:SetSize(w - self.__indent - 24, h)
end

local DTextEntry = derma.DefineControl("DTextEntry", "Text entry", {}, "Panel")
function DTextEntry:Init()
	self.keyboard_input = true
	self.__editable, self.__text, self.__caret = true, "", 0
	self.__placeholder, self.__numeric, self.__history = "", false, {}
	self.__update_on_type, self.__enter_allowed = false, true
	self:SetSize(100, 20)
	self.__paint_bg = true
	self.__cursor = "beam"
end
function DTextEntry:SetPlaceholderText(t) self.__placeholder = tostring(t or "") end
function DTextEntry:GetPlaceholderText() return self.__placeholder end
function DTextEntry:SetPlaceholderColor(c) self.__placeholder_color = c end
function DTextEntry:SetNumeric(b) self.__numeric = b == true end
function DTextEntry:GetNumeric() return self.__numeric end
function DTextEntry:SetUpdateOnType(b) self.__update_on_type = b == true end
function DTextEntry:GetUpdateOnType() return self.__update_on_type end
function DTextEntry:SetEnterAllowed(b) self.__enter_allowed = b ~= false end
function DTextEntry:GetEnterAllowed() return self.__enter_allowed end
function DTextEntry:SetTabbingDisabled(b) end
function DTextEntry:SetHighlightColor(c) end
function DTextEntry:SetCursorColor(c) end
function DTextEntry:SetDrawBorder(b) end
function DTextEntry:SetEditable(b) self.__editable = b ~= false end
function DTextEntry:IsEditable() return self.__editable end
function DTextEntry:GetInt() return math.floor(tonumber(self.__text) or 0) end
function DTextEntry:GetFloat() return tonumber(self.__text) or 0 end
function DTextEntry:SetText(t) self.__text = tostring(t == nil and "" or t); self.__caret = #self.__text end
function DTextEntry:SetValue(v)
	local old = self.__text
	self:SetText(v)
	if old ~= self.__text then self:OnValueChange(self.__text) end
end
function DTextEntry:GetValue() return self.__text end
function DTextEntry:AllowInput(ch) return false end
function DTextEntry:OnValueChange(v) end
function DTextEntry:OnChange() end
function DTextEntry:OnEnter(v) end
function DTextEntry:OnTextChanged() end
function DTextEntry:OnKeyCode(code) end
function DTextEntry:CheckNumeric(ch) return self.__numeric and not ch:match("^[%d%.%-]$") end
function DTextEntry:AddHistory(t) self.__history[#self.__history + 1] = t end
function DTextEntry:GetHistory() return self.__history end
function DTextEntry:SetHistoryEnabled(b) end
function DTextEntry:GetAutoComplete(t) end
function DTextEntry:SetCaretPos(i) self.__caret = math.max(0, math.min(i, #self.__text)) end
function DTextEntry:GetCaretPos() return self.__caret end
function DTextEntry:SelectAllText() self.__select_all = true end
function DTextEntry:SelectAllOnFocus(b) self.__select_on_focus = b == true end
function DTextEntry:SelectAll() self.__select_all = true end
function DTextEntry:GetSelectedText() return self.__select_all and self.__text or "" end
function DTextEntry:OnGetFocus() if self.__select_on_focus then self.__select_all = true end hook.Run("OnTextEntryGetFocus", self) end
function DTextEntry:OnLoseFocus() self.__select_all = false; hook.Run("OnTextEntryLoseFocus", self); if not self.__update_on_type then self:__commit() end end
function DTextEntry:__commit()
	if self.__text ~= self.__committed then
		self.__committed = self.__text
		self:OnValueChange(self.__text)
		self:ConVarChanged(self.__text)
	end
end
function DTextEntry:__insert(ch)
	if not self.__editable or not self.enabled then return end
	if self:CheckNumeric(ch) or self:AllowInput(ch) then return end
	if self.__max_chars and #self.__text >= self.__max_chars then return end
	if self.__select_all then self.__text, self.__caret, self.__select_all = "", 0, false end
	self.__text = self.__text:sub(1, self.__caret) .. ch .. self.__text:sub(self.__caret + 1)
	self.__caret = self.__caret + #ch
	self:OnTextChanged()
	self:OnChange()
	if self.__update_on_type then self:__commit() end
end
function DTextEntry:OnKeyChar(ch) self:__insert(ch) end
function DTextEntry:OnKeyCodeTyped(code)
	self:OnKeyCode(code)
	if code == KEY_ENTER or code == KEY_PAD_ENTER then
		if not self.__multiline then
			if self.__enter_allowed then
				self:__commit()
				self:AddHistory(self.__text)
				self:OnEnter(self.__text)
			end
			return true
		end
		self:__insert("\n")
		return true
	elseif code == KEY_BACKSPACE then
		if self.__select_all then self.__text, self.__caret, self.__select_all = "", 0, false
		elseif self.__caret > 0 then
			self.__text = self.__text:sub(1, self.__caret - 1) .. self.__text:sub(self.__caret + 1)
			self.__caret = self.__caret - 1
		end
		self:OnTextChanged(); self:OnChange()
		if self.__update_on_type then self:__commit() end
		return true
	elseif code == KEY_DELETE then
		self.__text = self.__text:sub(1, self.__caret) .. self.__text:sub(self.__caret + 2)
		self:OnTextChanged(); self:OnChange()
		return true
	elseif code == KEY_LEFT then self.__caret = math.max(0, self.__caret - 1); self.__select_all = false; return true
	elseif code == KEY_RIGHT then self.__caret = math.min(#self.__text, self.__caret + 1); self.__select_all = false; return true
	elseif code == KEY_HOME then self.__caret = 0; return true
	elseif code == KEY_END then self.__caret = #self.__text; return true
	elseif code == KEY_ESCAPE then self:KillFocus(); return true
	elseif code == KEY_TAB then return false end
	return true
end
function DTextEntry:OnMousePressed(code)
	self:RequestFocus()
	surface.SetFont(self:GetFont())
	local lx = (self:CursorPos()) - 3
	local best = #self.__text
	for i = 0, #self.__text do
		if surface.GetTextSize(self.__text:sub(1, i)) >= lx then best = i; break end
	end
	self.__caret, self.__select_all = best, false
end
function DTextEntry:Paint(w, h)
	if self.__paint_bg then
		surface.SetDrawColor(self.enabled and Color(255, 255, 255) or Color(220, 220, 220))
		surface.DrawRect(0, 0, w, h)
		surface.SetDrawColor(focused == self and SKIN.control_color_active or SKIN.colButtonBorder)
		surface.DrawOutlinedRect(0, 0, w, h)
	end
	surface.SetFont(self:GetFont())
	local _, th = surface.GetTextSize("W")
	local ty = math.floor(h / 2 - th / 2)
	if self.__text == "" and self.__placeholder ~= "" and focused ~= self then
		surface.SetTextColor(self.__placeholder_color or SKIN.colTextEntryTextPlaceholder)
		surface.SetTextPos(3, ty)
		surface.DrawText(self.__placeholder)
		return
	end
	if self.__select_all and focused == self then
		local tw = surface.GetTextSize(self.__text)
		surface.SetDrawColor(SKIN.colTextEntryTextHighlight)
		surface.DrawRect(3, ty, tw, th)
	end
	surface.SetTextColor(self.__text_color or SKIN.colTextEntryText)
	surface.SetTextPos(3, ty)
	surface.DrawText(self.__text)
	if focused == self and self.enabled and (SysTime() % 1) < 0.5 then
		local cx = surface.GetTextSize(self.__text:sub(1, self.__caret))
		surface.SetDrawColor(SKIN.colTextEntryTextCursor)
		surface.DrawRect(3 + cx, ty, 1, th)
	end
end

local DImage = derma.DefineControl("DImage", "Image", {}, "DPanel")
function DImage:Init() self.__paint_bg = false; self.__image_color = Color(255, 255, 255); self.__keep_aspect = false; self.mouse_input = false end
function DImage:SetImage(path, backup) self.__image = Material(path); self.__image_path = path end
function DImage:GetImage() return self.__image_path or "" end
function DImage:SetMaterial(m) self.__image = m end
function DImage:GetMaterial() return self.__image end
function DImage:SetImageColor(c) self.__image_color = c end
function DImage:GetImageColor() return self.__image_color end
function DImage:SetKeepAspect(b) self.__keep_aspect = b == true end
function DImage:GetKeepAspect() return self.__keep_aspect end
function DImage:SetFailsafeMatName() end
function DImage:SetOnViewMaterial(m) self:SetImage(m) end
function DImage:Unloaded() return false end
function DImage:SizeToContents() end
function DImage:PaintAt(x, y, w, h)
	if not self.__image then return end
	surface.SetMaterial(self.__image)
	surface.SetDrawColor(self.__image_color)
	surface.DrawTexturedRect(x, y, w, h)
end
function DImage:Paint(w, h) self:PaintAt(0, 0, w, h) end

local DProgress = derma.DefineControl("DProgress", "Progress bar", {}, "DPanel")
function DProgress:Init() self.__fraction = 0 end
function DProgress:SetFraction(f) self.__fraction = math.max(0, math.min(1, f or 0)) end
function DProgress:GetFraction() return self.__fraction end
function DProgress:Paint(w, h)
	surface.SetDrawColor(SKIN.bg_color_bright)
	surface.DrawRect(0, 0, w, h)
	surface.SetDrawColor(SKIN.control_color_active)
	surface.DrawRect(0, 0, math.floor(w * self.__fraction), h)
	surface.SetDrawColor(SKIN.colButtonBorder)
	surface.DrawOutlinedRect(0, 0, w, h)
end

-- DScrollPanel: a canvas moved by the wheel; the bar is drawn as a track on the right
local DScrollPanel = derma.DefineControl("DScrollPanel", "Scrolling panel", {}, "DPanel")
function DScrollPanel:Init()
	self.__paint_bg = false
	self.pnlCanvas = vgui.Create("Panel", self)
	self.pnlCanvas.mouse_input = true
	self.pnlCanvas.__scroll = self
	self.pnlCanvas.PerformLayout = function(c, w, h) self:InvalidateLayout() end
	self.VBar = { Enabled = true, Scroll = 0, SetScroll = function(b, s) b.Scroll = s end, GetScroll = function(b) return b.Scroll end, SetEnabled = function(b, e) b.Enabled = e end, SetHideButtons = function() end, AnimateTo = function(b, s) b.Scroll = s end, GetOffset = function(b) return -b.Scroll end, SetUp = function() end, AddScroll = function(b, d) b.Scroll = b.Scroll + d end, IsVisible = function(b) return b.__visible == true end, SetWide = function() end, GetWide = function() return 15 end }
	self.__scroll = 0
	self.__bar_w = 15
end
function DScrollPanel:GetCanvas() return self.pnlCanvas end
function DScrollPanel:GetVBar() return self.VBar end
function DScrollPanel:AddItem(p) p:SetParent(self.pnlCanvas) end
function DScrollPanel:Add(class) return vgui.Create(class, self.pnlCanvas) end
function DScrollPanel:Clear() self.pnlCanvas:Clear() end
function DScrollPanel:GetChildren() return self.pnlCanvas:GetChildren() end
function DScrollPanel:SetPadding(p) self.__pad = p end
function DScrollPanel:GetPadding() return self.__pad or 0 end
function DScrollPanel:InnerWidth() return self.pnlCanvas.w end
function DScrollPanel:Rebuild() self:InvalidateLayout() end
function DScrollPanel:ScrollToChild(c) local _, y = c:LocalToScreen(0, 0); local _, sy = self.pnlCanvas:LocalToScreen(0, 0); self.VBar.Scroll = y - sy; self:InvalidateLayout() end
function DScrollPanel:OnVScroll(off) self.pnlCanvas:SetPos(0, off) end
function DScrollPanel:OnMouseWheeled(d)
	if not self.VBar.__visible then return false end
	self.VBar.Scroll = math.max(0, math.min(self.VBar.Scroll - d * 30, self.pnlCanvas.h - self.h))
	self:InvalidateLayout()
	return true
end
function DScrollPanel:PerformLayout(w, h)
	local c = self.pnlCanvas
	c:__layout()
	local _, ch = c:ChildrenSize()
	local bl, bt, br, bb = table.unpack(c.__padding)
	ch = ch + bb
	self.VBar.__visible = ch > h
	c:SetSize(self.VBar.__visible and (w - self.__bar_w) or w, math.max(ch, h))
	self.VBar.Scroll = math.max(0, math.min(self.VBar.Scroll, math.max(0, ch - h)))
	self:OnVScroll(-self.VBar.Scroll)
	c.__invalid = true
	c:__layout()
end
function DScrollPanel:PaintOver(w, h)
	if not self.VBar.__visible then return end
	local ch = self.pnlCanvas.h
	surface.SetDrawColor(Color(0, 0, 0, 100))
	surface.DrawRect(w - self.__bar_w, 0, self.__bar_w, h)
	local grip = math.max(10, h * h / ch)
	local gy = (h - grip) * (self.VBar.Scroll / math.max(1, ch - h))
	surface.SetDrawColor(SKIN.control_color)
	surface.DrawRect(w - self.__bar_w + 2, gy, self.__bar_w - 4, grip)
end

-- DMenu / DMenuOption / DermaMenu(): a popup list at the cursor; a click elsewhere closes it
local DMenuOption = derma.DefineControl("DMenuOption", "Menu option", {}, "DButton")
function DMenuOption:Init()
	self.__align = 4
	self.__inset = { 30, 0 }
	self.__paint_bg = false
	self:SetTall(22)
	self.__checkable, self.__checked = false, false
end
function DMenuOption:SetMenu(m) self.__menu = m end
function DMenuOption:GetMenu() return self.__menu end
function DMenuOption:SetSubMenu(m) self.__sub = m end
function DMenuOption:AddSubMenu() local m = vgui.Create("DMenu", self.__menu and self.__menu.parent); m:SetVisible(false); m.__is_menu = true; self.__sub = m; return m end
function DMenuOption:GetSubMenu() return self.__sub end
function DMenuOption:SetIsCheckable(b) self.__checkable = b == true end
function DMenuOption:GetIsCheckable() return self.__checkable end
function DMenuOption:SetChecked(b) self.__checked = b == true end
function DMenuOption:GetChecked() return self.__checked end
function DMenuOption:ToggleCheck() self.__checked = not self.__checked; self:OnChecked(self.__checked) end
function DMenuOption:OnChecked(b) end
function DMenuOption:SetIcon(i) self.__icon = i end
function DMenuOption:GetIcon() return self.__icon end
function DMenuOption:SetDeleteSelf(b) self.__delete_self = b end
function DMenuOption:SetRadio(b) self.__radio = b end
function DMenuOption:OnMenuOpened(m) end
function DMenuOption:DoClickInternal()
	if self.__checkable then self:ToggleCheck() end
	local m = self.__menu
	if m and m.OnOptionSelected then m:OnOptionSelected(self) end
	if m and (self.__delete_self == nil or self.__delete_self) then CloseDermaMenus() end
end
function DMenuOption:Paint(w, h)
	if self:IsHovered() then
		surface.SetDrawColor(SKIN.control_color_active)
		surface.DrawRect(0, 0, w, h)
	end
	if self.__checkable and self.__checked then
		surface.SetDrawColor(Color(0, 0, 0))
		surface.DrawRect(9, h / 2 - 3, 6, 6)
	end
	self:__draw_text(w, h, self.__text_color or (self:IsHovered() and Color(255, 255, 255) or Color(0, 0, 0)))
end

local DMenu = derma.DefineControl("DMenu", "Menu", {}, "DScrollPanel")
local open_menus = {}
function DMenu:Init()
	self.__is_menu = true
	self.__delete_self = true
	self.__min_w, self.__max_h = 100, ScrH() - 40
	self.__options = {}
	self:SetVisible(false)
	self:SetSize(100, 10)
	self.mouse_input = true
	open_menus[#open_menus + 1] = self
end
function DMenu:SetMinimumWidth(w) self.__min_w = w end
function DMenu:GetMinimumWidth() return self.__min_w end
function DMenu:SetMaxHeight(h) self.__max_h = h end
function DMenu:GetMaxHeight() return self.__max_h end
function DMenu:SetDeleteSelf(b) self.__delete_self = b ~= false end
function DMenu:GetDeleteSelf() return self.__delete_self end
function DMenu:SetDrawColumn(b) self.__column = b end
function DMenu:SetDrawBorder(b) end
function DMenu:SetOpenSubMenu(m) end
function DMenu:GetOpenSubMenu() end
function DMenu:Hide() self:SetVisible(false) end
function DMenu:ChildCount() return #self.__options end
function DMenu:GetChild(i) return self.__options[i + 1] end
function DMenu:OnOptionSelected(opt) end
function DMenu:OptionSelected(opt, text) end
function DMenu:AddOption(text, fn)
	local o = vgui.Create("DMenuOption", self.pnlCanvas)
	o:SetMenu(self)
	o:SetText(text)
	if fn then o.DoClick = fn end
	o:Dock(TOP)
	self.__options[#self.__options + 1] = o
	self:InvalidateLayout()
	return o
end
function DMenu:AddCVar(text, cvar, value) local o = self:AddOption(text, function() RunConsoleCommand(cvar, value) end); o:SetIsCheckable(true); o:SetChecked(GetConVar(cvar) and GetConVar(cvar):GetString() == tostring(value)); return o end
function DMenu:AddSpacer()
	local s = vgui.Create("DPanel", self.pnlCanvas)
	s:SetTall(1)
	s:Dock(TOP)
	s:DockMargin(0, 2, 0, 2)
	s.__bg_color = Color(0, 0, 0, 100)
	s.Paint = function(_, w, h) surface.SetDrawColor(0, 0, 0, 100); surface.DrawRect(0, 0, w, h) end
	self.__options[#self.__options + 1] = s
	return s
end
function DMenu:AddSubMenu(text, fn)
	local o = self:AddOption(text, fn)
	local sub = o:AddSubMenu()
	o.OnCursorEntered = function() sub:Open(select(1, o:LocalToScreen(o.w, 0))) end
	o.__delete_self = false
	return sub, o
end
function DMenu:AddPanel(p) p:SetParent(self.pnlCanvas); p:Dock(TOP); self.__options[#self.__options + 1] = p end
function DMenu:Open(x, y, skip, owner)
	x, y = x or cursor_x, y or cursor_y
	self:SetParent(world_panel)
	-- the game's RegisterDermaMenuForClose: the open menu closes on a click outside it
	local w = self.__min_w
	surface.SetFont("DermaDefault")
	local total = 0
	for _, o in ipairs(self.__options) do
		if o.GetText then local tw = surface.GetTextSize(o:GetText()); w = math.max(w, tw + 40) end
		total = total + o.h + (o.__margin and (o.__margin[2] + o.__margin[4]) or 0)
	end
	local h = math.min(total + 2, self.__max_h)
	if x + w > ScrW() then x = ScrW() - w end
	if y + h > ScrH() then y = ScrH() - h end
	self:SetPos(x, y)
	self:SetSize(w, h)
	self:SetVisible(true)
	self:MakePopup()
	for _, o in ipairs(self.__options) do if o.OnMenuOpened then o:OnMenuOpened(self) end end
end
function DMenu:Paint(w, h)
	surface.SetDrawColor(SKIN.colMenuBG)
	surface.DrawRect(0, 0, w, h)
	surface.SetDrawColor(SKIN.colMenuBorder)
	surface.DrawOutlinedRect(0, 0, w, h)
end
function DMenu:OnRemove() for i = #open_menus, 1, -1 do if open_menus[i] == self then table.remove(open_menus, i) end end end
function DermaMenu(keep, parent)
	if not keep then CloseDermaMenus() end
	local m = vgui.Create("DMenu", parent)
	return m
end
function CloseDermaMenus()
	for i = #open_menus, 1, -1 do
		local m = open_menus[i]
		if m.__valid and m.__delete_self then m:Remove() elseif m.__valid then m:SetVisible(false) end
	end
	hook.Run("CloseDermaMenus")
end
function RegisterDermaMenuForClose(m) open_menus[#open_menus + 1] = m end

-- DComboBox: a button that opens a DMenu with the choices
local DComboBox = derma.DefineControl("DComboBox", "Combo box", {}, "DButton")
function DComboBox:Init()
	self.__choices, self.__data, self.__selected = {}, {}, nil
	self.__align = 4
	self.__inset = { 8, 0 }
	self:SetText("")
	self:SetTall(22)
	self.__sort = true
end
function DComboBox:Clear() self.__choices, self.__data, self.__selected = {}, {}, nil; self:SetText("") end
function DComboBox:AddChoice(text, data, select, icon)
	self.__choices[#self.__choices + 1] = text
	self.__data[#self.__choices] = data
	if select then self:ChooseOptionID(#self.__choices) end
	return #self.__choices
end
function DComboBox:RemoveChoice(i) table.remove(self.__choices, i); table.remove(self.__data, i) end
function DComboBox:GetOptionText(i) return self.__choices[i] end
function DComboBox:GetOptionData(i) return self.__data[i] end
function DComboBox:GetOptionTextByData(d) for i, v in ipairs(self.__data) do if v == d then return self.__choices[i] end end return d end
function DComboBox:GetSelectedID() return self.__selected end
function DComboBox:GetSelected() if self.__selected then return self.__choices[self.__selected], self.__data[self.__selected] end end
function DComboBox:GetValue() return self:GetText() end
function DComboBox:SetValue(v) self:SetText(v) end
function DComboBox:SetSortItems(b) self.__sort = b ~= false end
function DComboBox:ChooseOption(text, i)
	self.__selected = i
	self:SetText(text)
	self:OnSelect(i, text, self.__data[i])
	self:ConVarChanged(self.__data[i] or text)
end
function DComboBox:ChooseOptionID(i) local t = self.__choices[i]; if t then self:ChooseOption(t, i) end end
function DComboBox:ChooseOptionData(d) for i, v in ipairs(self.__data) do if v == d then self:ChooseOptionID(i) return end end end
function DComboBox:OnSelect(i, text, data) end
function DComboBox:OnMenuOpened(m) end
function DComboBox:IsMenuOpen() return self.__menu and self.__menu.__valid and self.__menu.visible end
function DComboBox:CloseMenu() if self.__menu and self.__menu.__valid then self.__menu:Remove() end self.__menu = nil end
function DComboBox:OpenMenu()
	if #self.__choices == 0 then return end
	self:CloseMenu()
	local m = DermaMenu(false, self)
	m:SetMinimumWidth(self.w)
	local order = {}
	for i = 1, #self.__choices do order[i] = i end
	if self.__sort then table.sort(order, function(a, b) return tostring(self.__choices[a]) < tostring(self.__choices[b]) end) end
	for _, i in ipairs(order) do
		m:AddOption(self.__choices[i], function() self:ChooseOption(self.__choices[i], i) end)
	end
	local x, y = self:LocalToScreen(0, self.h)
	m:Open(x, y)
	self.__menu = m
	self:OnMenuOpened(m)
end
function DComboBox:DoClick() if self:IsMenuOpen() then self:CloseMenu() else self:OpenMenu() end end
function DComboBox:Paint(w, h)
	surface.SetDrawColor(Color(255, 255, 255))
	surface.DrawRect(0, 0, w, h)
	surface.SetDrawColor(SKIN.colButtonBorder)
	surface.DrawOutlinedRect(0, 0, w, h)
	self:__draw_text(w, h, self.__text_color or Color(0, 0, 0))
	-- the drop arrow at the right (Marlett "6" in the game): a small triangle of rects
	surface.SetDrawColor(Color(0, 0, 0))
	for i = 0, 3 do surface.DrawRect(w - 16 + i, h / 2 - 2 + i, 8 - i * 2, 1) end
end

-- DNumSlider: label, text entry and a slider on one row
local DSlider = derma.DefineControl("DSlider", "Slider", {}, "DPanel")
function DSlider:Init()
	self.__paint_bg = false
	self.__x, self.__y = 0, 0.5
	self.__lock_y = 0.5
	self.__notches = 0
	self.__dragging = false
	self.Knob = vgui.Create("DButton", self)
	self.Knob:SetText("")
	self.Knob:SetSize(15, 15)
	self.Knob.mouse_input = false
end
function DSlider:SetSlideX(x) self.__x = math.max(0, math.min(1, x)); self:InvalidateLayout() end
function DSlider:GetSlideX() return self.__x end
function DSlider:SetSlideY(y) self.__y = y end
function DSlider:GetSlideY() return self.__y end
function DSlider:SetLockY(y) self.__lock_y = y end
function DSlider:GetLockY() return self.__lock_y end
function DSlider:SetLockX(x) self.__lock_x = x end
function DSlider:GetLockX() return self.__lock_x end
function DSlider:SetNotches(n) self.__notches = n or 0 end
function DSlider:GetNotches() return self.__notches end
function DSlider:SetTrapInside(b) end
function DSlider:SetDragging(b) self.__dragging = b end
function DSlider:IsEditing() return self.__dragging end
function DSlider:OnValueChanged(x, y) end
function DSlider:OnValuesChangedInternal() self:OnValueChanged(self.__x, self.__y) end
function DSlider:TranslateValues(x, y) return x, y end
function DSlider:OnMousePressed(code)
	self.__dragging = true
	self:MouseCapture(true)
	self:Think()
end
function DSlider:OnMouseReleased() self.__dragging = false; self:MouseCapture(false) end
function DSlider:Think()
	if not self.__dragging then return end
	local lx = self:CursorPos()
	local x = math.max(0, math.min(1, (lx - 7) / math.max(1, self.w - 15)))
	x = self:TranslateValues(x, self.__lock_y)
	if x ~= self.__x then self.__x = x; self:InvalidateLayout(); self:OnValuesChangedInternal() end
end
function DSlider:PerformLayout(w, h)
	self.Knob:SetPos(math.floor(self.__x * (w - 15)), math.floor(h / 2 - 7))
end
function DSlider:Paint(w, h)
	surface.SetDrawColor(SKIN.colNumSliderNotch)
	surface.DrawRect(7, h / 2 - 1, w - 14, 2)
	for i = 1, self.__notches - 1 do
		local nx = 7 + (w - 14) * i / self.__notches
		surface.DrawRect(nx, h / 2 - 4, 1, 8)
	end
end

local DNumSlider = derma.DefineControl("DNumSlider", "Numeric slider", {}, "DPanel")
function DNumSlider:Init()
	self.__paint_bg = false
	self.__min, self.__max, self.__decimals = 0, 1, 2
	self.Label = vgui.Create("DLabel", self)
	self.Label:SetText("")
	self.TextArea = vgui.Create("DTextEntry", self)
	self.TextArea:SetNumeric(true)
	self.TextArea:SetUpdateOnType(true)
	self.TextArea:SetWide(45)
	self.TextArea.__paint_bg = false
	self.TextArea.__text_color = Color(255, 255, 255)
	self.TextArea.OnValueChange = function(_, v) local n = tonumber(v); if n then self:SetValue(n) end end
	self.Slider = vgui.Create("DSlider", self)
	self.Slider.TranslateValues = function(_, x, y) return self:TranslateSliderValues(x, y) end
	self.Slider.OnValueChanged = function(_, x) self:SetValue(self.__min + (self.__max - self.__min) * x) end
	self.__value = 0
	self:SetTall(32)
	self:UpdateNotches()
end
function DNumSlider:SetMin(m) self.__min = tonumber(m) or 0; self:UpdateNotches() end
function DNumSlider:SetMax(m) self.__max = tonumber(m) or 1; self:UpdateNotches() end
function DNumSlider:GetMin() return self.__min end
function DNumSlider:GetMax() return self.__max end
function DNumSlider:SetMinMax(a, b) self:SetMin(a); self:SetMax(b) end
function DNumSlider:GetRange() return self.__max - self.__min end
function DNumSlider:SetDecimals(d) self.__decimals = d or 0; self:UpdateNotches() end
function DNumSlider:GetDecimals() return self.__decimals end
function DNumSlider:SetText(t) self.Label:SetText(t) end
function DNumSlider:GetText() return self.Label:GetText() end
function DNumSlider:SetDark(b) self.Label:SetDark(b) end
function DNumSlider:SetDefaultValue(v) self.__default = tonumber(v) end
function DNumSlider:GetDefaultValue() return self.__default end
function DNumSlider:ResetToDefaultValue() if self.__default then self:SetValue(self.__default) end end
function DNumSlider:GetTextArea() return self.TextArea end
function DNumSlider:IsEditing() return self.Slider:IsEditing() or self.TextArea:IsEditing() end
function DNumSlider:IsHovered() return hovered == self or self:IsChildHovered() end
function DNumSlider:UpdateNotches()
	local range = self.__max - self.__min
	if range <= 0 then self.Slider:SetNotches(0) return end
	local n = range / (10 ^ -(self.__decimals or 0))
	self.Slider:SetNotches(n <= 30 and math.floor(n) or 0)
end
function DNumSlider:TranslateSliderValues(x, y)
	local v = self.__min + (self.__max - self.__min) * x
	local p = 10 ^ (self.__decimals or 0)
	v = math.floor(v * p + 0.5) / p
	return (v - self.__min) / math.max(1e-9, self.__max - self.__min), y
end
function DNumSlider:SetValue(v)
	v = tonumber(v)
	if not v then return end
	v = math.max(self.__min, math.min(self.__max, v))
	local p = 10 ^ (self.__decimals or 0)
	v = math.floor(v * p + 0.5) / p
	if v == self.__value then return end
	self.__value = v
	self.Slider.__x = (v - self.__min) / math.max(1e-9, self.__max - self.__min)
	self.Slider:InvalidateLayout()
	if not self.TextArea:IsEditing() then self.TextArea:SetText(self.__decimals > 0 and string.format("%." .. self.__decimals .. "f", v) or tostring(math.floor(v))) end
	self:ValueChanged(v)
	self:OnValueChanged(v)
	self:ConVarChanged(v)
end
function DNumSlider:GetValue() return self.__value end
function DNumSlider:ValueChanged(v) end
function DNumSlider:OnValueChanged(v) end
function DNumSlider:SetConVar(c)
	self.__convar = c
	local cv = GetConVar(c)
	if cv then self:SetValue(cv:GetFloat()) end
end
function DNumSlider:PerformLayout(w, h)
	self.Label:SetPos(0, 0)
	self.Label:SetSize(w * 0.4, h)
	self.TextArea:SetPos(w - 45, h / 2 - 10)
	self.TextArea:SetSize(45, 20)
	self.Slider:SetPos(w * 0.4, 0)
	self.Slider:SetSize(w * 0.6 - 45, h)
end

-- the Derma_* dialogs
function Derma_Message(text, title, button)
	local f = vgui.Create("DFrame")
	f:SetTitle(title or "Message")
	local l = vgui.Create("DLabel", f)
	l:SetText(text or "")
	l:SizeToContents()
	l:SetPos(10, 30)
	local b = vgui.Create("DButton", f)
	b:SetText(button or "OK")
	b:SizeToContents()
	b.DoClick = function() f:Close() end
	f:SetSize(math.max(l.w + 20, b.w + 20, 150), l.h + b.h + 60)
	b:SetPos(f.w / 2 - b.w / 2, f.h - b.h - 10)
	f:Center()
	f:MakePopup()
	return f
end
function Derma_Query(text, title, ...)
	local f = vgui.Create("DFrame")
	f:SetTitle(title or "Message")
	local l = vgui.Create("DLabel", f)
	l:SetText(text or "")
	l:SizeToContents()
	l:SetPos(10, 30)
	local args = { ... }
	local bw, buttons = 0, {}
	for i = 1, #args, 2 do
		local b = vgui.Create("DButton", f)
		b:SetText(args[i])
		b:SizeToContents()
		local fn = args[i + 1]
		b.DoClick = function() f:Close(); if fn then fn() end end
		buttons[#buttons + 1] = b
		bw = bw + b.w + 5
	end
	f:SetSize(math.max(l.w + 20, bw + 20, 150), l.h + 30 + 50)
	local x = f.w / 2 - bw / 2
	for _, b in ipairs(buttons) do b:SetPos(x, f.h - b.h - 10); x = x + b.w + 5 end
	f:Center()
	f:MakePopup()
	return f
end
function Derma_StringRequest(title, text, default, enter, cancel, enter_text, cancel_text)
	local f = vgui.Create("DFrame")
	f:SetTitle(title or "Message")
	f:SetSize(300, 110)
	local l = vgui.Create("DLabel", f)
	l:SetText(text or "")
	l:SizeToContents()
	l:SetPos(10, 30)
	local e = vgui.Create("DTextEntry", f)
	e:SetText(default or "")
	e:SetPos(10, 50)
	e:SetSize(280, 20)
	e.OnEnter = function() f:Close(); if enter then enter(e:GetValue()) end end
	local ok = vgui.Create("DButton", f)
	ok:SetText(enter_text or "OK")
	ok:SetSize(60, 20)
	ok:SetPos(f.w / 2 - 65, 80)
	ok.DoClick = e.OnEnter
	local no = vgui.Create("DButton", f)
	no:SetText(cancel_text or "Cancel")
	no:SetSize(60, 20)
	no:SetPos(f.w / 2 + 5, 80)
	no.DoClick = function() f:Close(); if cancel then cancel(e:GetValue()) end end
	f:Center()
	f:MakePopup()
	e:RequestFocus()
	return f
end
function Derma_DrawBackgroundBlur(panel, start)
	surface.SetDrawColor(0, 0, 0, 100)
	local x, y = panel:LocalToScreen(0, 0)
	local ox, oy = surface.__get_origin()
	surface.__origin(0, 0)
	surface.DrawRect(0, 0, ScrW(), ScrH())
	surface.__origin(ox, oy)
end
function Derma_Anim(name, panel, fn)
	local a = { Started = false, Finished = false, Start = function(s, len, data) s.Started, s.Finished, s.StartTime, s.EndTime, s.Data = true, false, SysTime(), SysTime() + (len or 0), data end, Active = function(s) return s.Started and not s.Finished end, Run = function(s, delta) end, Think = function(s)
		if not s.Started or s.Finished then return end
		local f = s.EndTime > s.StartTime and math.min(1, (SysTime() - s.StartTime) / (s.EndTime - s.StartTime)) or 1
		fn(panel, s, f, s.Data)
		if f >= 1 then s.Finished = true end
	end }
	return a
end
function Derma_Hook(panel, fname, hname, typename)
	panel[fname] = function(p, ...) return derma.SkinHook(hname, typename, p, ...) end
end
function Derma_Install_Convar_Functions(p)
	p.SetConVar = function(s, c) s.__convar = c end
	p.ConVarChanged = function(s, v) if s.__convar then RunConsoleCommand(s.__convar, tostring(v)) end end
	p.ConVarStringThink = function() end
	p.ConVarNumberThink = function() end
end

-- `FindMetaTable("Panel")` and the control tables: addons extend them (the game's
-- `vgui.GetControlTable` returns the same tables)
__glua_classes.Panel = Panel
for name, tbl in pairs(classes) do __glua_classes[name] = tbl end
-- documented panels not built yet: creatable as a plain DPanel so addons keep running
for _, n in ipairs({ "DColorMixer", "DColorPalette", "DColorButton", "DColorCube", "DAlphaBar", "DRGBPicker", "DColorCombo", "DListView", "DListView_Line", "DListView_Column", "DListBox", "DListBoxItem", "DTree", "DTree_Node", "DPropertySheet", "DTab", "DCollapsibleCategory", "DCategoryList", "DForm", "DIconLayout", "DGrid", "DTooltip", "DHTML", "DHTMLControls", "DModelPanel", "DModelSelect", "DAdjustableModelPanel", "DImageButton", "DSpawnIcon", "DIconBrowser", "DFileBrowser", "DEntityProperties", "DProperties", "DProperty_Generic", "DProperty_Boolean", "DProperty_Combo", "DProperty_Float", "DProperty_Int", "DProperty_VectorColor", "DNumberWang", "DNumberScratch", "DNumPad", "DBinder", "DButtonSelect", "DHorizontalDivider", "DHorizontalScroller", "DVerticalDivider", "DDragBase", "DDrawer", "DExpandButton", "DKillIcon", "DLabelEditable", "DLabelURL", "DMenuBar", "DMenuOptionCVar", "DNotify", "DPanelList", "DPanelOverlay", "DPanelSelect", "DProgressBar", "DScrollBarGrip", "DShape", "DSizeToContents", "DSprite", "DTextEditor", "DTileLayout", "DTreeScroller", "DTree_Node_Button", "DVScrollBar", "ContentIcon", "ContentHeader", "ContentContainer", "ContentSidebar", "ContentSidebarToolbox", "SpawnIcon", "SpawnmenuContentPanel", "ControlPanel", "ControlPresets", "PresetEditor", "PropSelect", "MatSelect", "ImageCheckBox", "Material", "AvatarImage", "RichText", "RadioButton", "CheckButton", "Frame", "Label", "Button", "TextEntry", "HTML", "Slider", "ImagePanel", "ScrollBar", "ScrollBarGrip", "Divider", "GModNotify", "ToolMenu", "ToolPanel", "DAlphaBar", "SlideBar" }) do
	if not classes[n] then vgui.Register(n, {}, "DPanel") end
end
for name, tbl in pairs(classes) do __glua_classes[name] = tbl end
