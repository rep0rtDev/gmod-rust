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
	if not c then return end
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
	DScrollPanel.Init(self)
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

------------------------------------------------------------------------ more controls
-- small documented accessors the base classes share
function DPanel:GetTabbingDisabled() return self.__no_tab == true end
function DPanel:SetTabbingDisabled(b) self.__no_tab = b == true end
function DPanel:UpdateColours() end
function DLabel:GetTextStyleColor() return self.__text_color end
function DLabel:SetTextStyleColor(c) self.__text_color = c end
function DLabel:GetDoubleClickingEnabled() return true end
function DLabel:GetIsMenu() return self.__is_menu == true end
function DLabel:SetIsMenu(b) self.__is_menu = b == true end
function DButton:GetDrawBorder() return true end
function DButton:SetMaterial(m) self.__icon = m end
function DFrame:GetIsMenu() return self.__is_menu == true end
function DFrame:SetIsMenu(b) self.__is_menu = b == true end
function DFrame:Center() Panel.Center(self) end
function DTextEntry:GetTextColor() return self.__text_color end
function DTextEntry:SetTextColor(c) self.__text_color = c end
function DTextEntry:SetFont(f) self.__font = f end
function DTextEntry:GetCursorColor() return SKIN.colTextEntryTextCursor end
function DTextEntry:GetHighlightColor() return SKIN.colTextEntryTextHighlight end
function DTextEntry:GetPlaceholderColor() return self.__placeholder_color or SKIN.colTextEntryTextPlaceholder end
function DTextEntry:GetTabbingDisabled() return self.__no_tab == true end
function DTextEntry:SetTabbingDisabled(b) self.__no_tab = b == true end
function DTextEntry:GetHistoryEnabled() return self.__history_enabled == true end
function DTextEntry:SetHistoryEnabled(b) self.__history_enabled = b == true end
function DTextEntry:GetDrawBackground() return self.__paint_bg ~= false end
function DTextEntry:SetDrawBackground(b) self.__paint_bg = b ~= false end
function DTextEntry:GetPaintBackground() return self.__paint_bg ~= false end
function DTextEntry:SetPaintBackground(b) self.__paint_bg = b ~= false end
function DTextEntry:GetDrawBorder() return true end
function DTextEntry:GetDisabled() return not self.enabled end
function DTextEntry:SetDisabled(b) self.enabled = not b end
function DTextEntry:IsEditing() return focused == self end
function DTextEntry:OpenAutoComplete(list) end
function DTextEntry:UpdateConvarValue() if self.__convar then local cv = GetConVar(self.__convar); if cv then self:SetText(cv:GetString()) end end end
function DTextEntry:UpdateFromHistory(d) local n = #self.__history; if n == 0 then return end self.__hist_i = math.max(1, math.min(n, (self.__hist_i or n + 1) + d)); self:SetText(self.__history[self.__hist_i]) end
function DTextEntry:UpdateFromMenu() end
function DMenu:GetDrawBorder() return true end
function DMenu:GetDrawColumn() return self.__column == true end
function DMenu:HighlightItem(item) self.__highlight = item end
function DMenu:ClearHighlights() self.__highlight = nil end
function DMenu:OpenSubMenu(item, menu) if self.__open_sub and self.__open_sub ~= menu then self:CloseSubMenu(self.__open_sub) end self.__open_sub = menu; if menu then menu:Open(item:LocalToScreen(item.w, 0)) end end
function DMenu:CloseSubMenu(menu) if menu and menu.__valid then menu:Hide() end if self.__open_sub == menu then self.__open_sub = nil end end
function DMenu:OptionSelectedInternal(opt) self:OptionSelected(opt, opt:GetText()) end
function DMenuOption:GetRadio() return self.__radio == true end
function DComboBox:AddSpacer() self.__choices[#self.__choices + 1] = "-"; self.__data[#self.__choices] = nil; self.__spacers = self.__spacers or {}; self.__spacers[#self.__choices] = true end
function DComboBox:CheckConVarChanges() if self.__convar then local cv = GetConVar(self.__convar); if cv and cv:GetString() ~= self.__convar_seen then self.__convar_seen = cv:GetString(); self:ChooseOptionData(self.__convar_seen) end end end
function DComboBox:GetSortItems() return self.__sort end
function DSlider:GetDragging() return self.__dragging end
function DSlider:GetTrapInside() return true end
function DSlider:GetNotchColor() return self.__notch_color or SKIN.colNumSliderNotch end
function DSlider:SetNotchColor(c) self.__notch_color = c end
function DSlider:SetNumSlider(n) self.__numslider = n end
function DSlider:GetNumSlider() return self.__numslider end
function DSlider:SetBackground(m) self.__background = m end
function DSlider:SetImage(m) self.__background = m end
function DSlider:SetImageColor(c) self.__image_color = c end
function DSlider:SetConVarX(c) self.__convar_x = c end
function DSlider:SetConVarY(c) self.__convar_y = c end
function DSlider:ConVarXNumberThink() end
function DSlider:ConVarYNumberThink() end
function DSlider:ResetToDefaultValue() if self.__numslider then self.__numslider:ResetToDefaultValue() end end
function DImage:GetMatName() return self.__image_path or "" end
function DImage:SetMatName(n) self.__image_path = n end
function DImage:GetFailsafeMatName() return self.__failsafe or "" end
function DImage:SetFailsafeMatName(n) self.__failsafe = n end
function DImage:LoadMaterial() if self.__image_path then self.__image = Material(self.__image_path) end end
function DImage:DoLoadMaterial() self:LoadMaterial() end
function DImage:FixVertexLitMaterial() end
function DScrollPanel:SetCanvas(c) self.pnlCanvas = c; c:SetParent(self) end
function DScrollPanel:PerformLayoutInternal() self:PerformLayout(self.w, self.h) end

-- DVScrollBar: the scroll panel's real bar (track, grip, up / down buttons)
local DVScrollBar = derma.DefineControl("DVScrollBar", "Vertical scroll bar", {}, "DPanel")
function DVScrollBar:Init()
	self.__paint_bg = false
	self.Offset, self.Scroll, self.CanvasSize, self.BarSize = 0, 0, 1, 1
	self.__hide_buttons = false
	self:SetWide(15)
	self.btnUp = vgui.Create("DButton", self); self.btnUp:SetText(""); self.btnUp.DoClick = function() self:AddScroll(-1) end
	self.btnDown = vgui.Create("DButton", self); self.btnDown:SetText(""); self.btnDown.DoClick = function() self:AddScroll(1) end
	self.btnGrip = vgui.Create("DButton", self); self.btnGrip:SetText("")
	self.btnGrip.OnMousePressed = function(g) g.__drag = select(2, g:CursorPos()); g:MouseCapture(true) end
	self.btnGrip.OnMouseReleased = function(g) g.__drag = nil; g:MouseCapture(false) end
	self.btnGrip.Think = function(g)
		if not g.__drag then return end
		local _, ly = self:CursorPos()
		local track = self.h - (self.__hide_buttons and 0 or self.w * 2)
		local f = (ly - (self.__hide_buttons and 0 or self.w) - g.__drag) / math.max(1, track - g.h)
		self:SetScroll(f * (self.CanvasSize - self.BarSize))
	end
end
function DVScrollBar:SetEnabled(b) self.enabled = b ~= false; self:SetVisible(self.enabled) end
function DVScrollBar:SetHideButtons(b) self.__hide_buttons = b == true; self:InvalidateLayout() end
function DVScrollBar:GetHideButtons() return self.__hide_buttons end
function DVScrollBar:SetUp(bar, canvas) self.BarSize, self.CanvasSize = bar, math.max(canvas - bar, 1); self:SetEnabled(canvas > bar); self:InvalidateLayout() end
function DVScrollBar:BarScale() return self.BarSize / (self.CanvasSize + self.BarSize) end
function DVScrollBar:GetScroll() return self.Scroll end
function DVScrollBar:SetScroll(s)
	s = math.max(0, math.min(s, self.CanvasSize))
	if s == self.Scroll then return end
	self.Scroll = s
	self:InvalidateLayout()
	if self.parent and self.parent.OnVScroll then self.parent:OnVScroll(self:GetOffset()) end
end
function DVScrollBar:AddScroll(d) local old = self.Scroll; self:SetScroll(self.Scroll + d * 25); return old ~= self.Scroll end
function DVScrollBar:AnimateTo(s, len) self:SetScroll(s) end
function DVScrollBar:GetOffset() return -self.Scroll end
function DVScrollBar:Grip() end
function DVScrollBar:OnMouseWheeled(d) return self:AddScroll(-d) end
function DVScrollBar:OnMousePressed() local _, ly = self:CursorPos(); if ly < self.btnGrip.y then self:AddScroll(-4) else self:AddScroll(4) end end
function DVScrollBar:PerformLayout(w, h)
	local bh = self.__hide_buttons and 0 or w
	self.btnUp:SetVisible(not self.__hide_buttons); self.btnDown:SetVisible(not self.__hide_buttons)
	self.btnUp:SetPos(0, 0); self.btnUp:SetSize(w, bh)
	self.btnDown:SetPos(0, h - bh); self.btnDown:SetSize(w, bh)
	local track = h - bh * 2
	local grip = math.max(10, track * self:BarScale())
	local gy = bh + (track - grip) * (self.Scroll / math.max(1, self.CanvasSize))
	self.btnGrip:SetPos(0, gy); self.btnGrip:SetSize(w, grip)
end
function DVScrollBar:Paint(w, h) surface.SetDrawColor(Color(0, 0, 0, 100)); surface.DrawRect(0, 0, w, h) end

-- DListView: columns, lines, selection, sorting
local DListView_Column = derma.DefineControl("DListView_Column", "List view column", {}, "DPanel")
function DListView_Column:Init() self.__paint_bg = false; self.__min_w, self.__max_w, self.__fixed_w = 10, 32000, nil; self.__desc = false; self.__align = 5; self.Header = vgui.Create("DButton", self); self.Header:SetText(""); self.Header.DoClick = function() self:DoClick() end; self.Header.DoRightClick = function() self:DoRightClick() end end
function DListView_Column:SetName(n) self.Header:SetText(n) end
function DListView_Column:GetName() return self.Header:GetText() end
function DListView_Column:SetColumnID(i) self.__id = i end
function DListView_Column:GetColumnID() return self.__id end
function DListView_Column:SetDescending(b) self.__desc = b == true end
function DListView_Column:GetDescending() return self.__desc end
function DListView_Column:SetMinWidth(w) self.__min_w = w end
function DListView_Column:GetMinWidth() return self.__min_w end
function DListView_Column:SetMaxWidth(w) self.__max_w = w end
function DListView_Column:GetMaxWidth() return self.__max_w end
function DListView_Column:SetFixedWidth(w) self.__fixed_w = w; self.__min_w, self.__max_w = w, w; self:SetWide(w) end
function DListView_Column:GetFixedWidth() return self.__fixed_w end
function DListView_Column:SetWidth(w) self:SetWide(math.max(self.__min_w, math.min(self.__max_w, w))) end
function DListView_Column:ResizeColumn(d) self:SetWidth(self.w + d) end
function DListView_Column:SetTextAlign(a) self.__align = a; self.Header:SetContentAlignment(a) end
function DListView_Column:GetTextAlign() return self.__align end
function DListView_Column:DoClick() local lv = self.parent and self.parent.__listview; if lv and lv.__sortable then lv:SortByColumn(self.__id, self.__desc); self.__desc = not self.__desc end end
function DListView_Column:DoRightClick() end
function DListView_Column:PerformLayout(w, h) self.Header:SetPos(0, 0); self.Header:SetSize(w, h) end

local DListView_Line = derma.DefineControl("DListView_Line", "List view line", {}, "Panel")
function DListView_Line:Init() self.Columns, self.Data, self.__alt, self.__id = {}, {}, false, 0; self.mouse_input = true end
function DListView_Line:SetListView(lv) self.__listview = lv end
function DListView_Line:GetListView() return self.__listview end
function DListView_Line:SetID(i) self.__id = i end
function DListView_Line:GetID() return self.__id end
function DListView_Line:SetAltLine(b) self.__alt = b == true end
function DListView_Line:GetAltLine() return self.__alt end
function DListView_Line:SetSelected(b) self.__selected = b == true end
function DListView_Line:IsLineSelected() return self.__selected == true end
function DListView_Line:SetColumnText(i, text)
	local c = self.Columns[i]
	if not c then
		c = vgui.Create("DLabel", self)
		c:SetText("")
		self.Columns[i] = c
	end
	if type(text) == "table" and text.IsValid then text:SetParent(self); self.Columns[i] = text; c:Remove(); return text end
	c:SetText(tostring(text == nil and "" or text))
	return c
end
function DListView_Line:GetColumnText(i) local c = self.Columns[i]; return c and c:GetText() or "" end
function DListView_Line:GetValue(i) return self:GetColumnText(i) end
function DListView_Line:SetValue(i, v) return self:SetColumnText(i, v) end
function DListView_Line:SetSortValue(i, v) self.Data[i] = v end
function DListView_Line:GetSortValue(i) return self.Data[i] end
function DListView_Line:DataLayout(lv)
	local x = 0
	for i, col in ipairs(lv.Columns) do
		local c = self.Columns[i]
		if c then c:SetPos(x + 3, 0); c:SetSize(col.w - 6, self.h); c:SetContentAlignment(col.__align or 4) end
		x = x + col.w
	end
end
function DListView_Line:OnSelect() end
function DListView_Line:OnRightClick() end
function DListView_Line:OnMousePressed(code)
	local lv = self.__listview
	if not lv then return end
	if code == MOUSE_RIGHT then lv:OnRowRightClick(self.__id, self); self:OnRightClick(); return end
	local now = SysTime()
	if self.__last_click and now - self.__last_click < 0.3 then lv:DoDoubleClick(self.__id, self); self.__last_click = nil; return end
	self.__last_click = now
	lv:OnClickLine(self, true)
end
function DListView_Line:Paint(w, h)
	if self.__selected then surface.SetDrawColor(SKIN.control_color_active)
	elseif self:IsHovered() then surface.SetDrawColor(Color(255, 255, 255, 100))
	elseif self.__alt then surface.SetDrawColor(Color(240, 240, 240))
	else surface.SetDrawColor(Color(255, 255, 255)) end
	surface.DrawRect(0, 0, w, h)
end

local DListView = derma.DefineControl("DListView", "List view", {}, "DPanel")
function DListView:Init()
	self.Columns, self.Lines, self.Sorted = {}, {}, {}
	self.__header_h, self.__data_h, self.__hide_headers, self.__multi, self.__sortable = 16, 17, false, false, true
	self.__dirty = true
	self.pnlCanvas = vgui.Create("Panel", self)
	self.pnlCanvas.mouse_input = true
	self.VBar = vgui.Create("DVScrollBar", self)
	self.VBar:SetEnabled(false)
	self.__paint_bg = true
	self.__bg_color = Color(255, 255, 255)
end
function DListView:GetCanvas() return self.pnlCanvas end
function DListView:SetHeaderHeight(h) self.__header_h = h; self:SetDirty(true) end
function DListView:GetHeaderHeight() return self.__header_h end
function DListView:SetDataHeight(h) self.__data_h = h; self:SetDirty(true) end
function DListView:GetDataHeight() return self.__data_h end
function DListView:GetInnerTall() return self.h - (self.__hide_headers and 0 or self.__header_h) end
function DListView:SetHideHeaders(b) self.__hide_headers = b == true; self:SetDirty(true) end
function DListView:GetHideHeaders() return self.__hide_headers end
function DListView:SetMultiSelect(b) self.__multi = b == true end
function DListView:GetMultiSelect() return self.__multi end
function DListView:SetSortable(b) self.__sortable = b ~= false end
function DListView:GetSortable() return self.__sortable end
function DListView:SetDirty(b) self.__dirty = b == true; if b then self:InvalidateLayout() end end
function DListView:GetDirty() return self.__dirty end
function DListView:DisableScrollbar() self.VBar:SetEnabled(false); self.__no_bar = true end
function DListView:AddColumn(name, pos)
	local c = vgui.Create("DListView_Column", self)
	c:SetName(name)
	c.__listview = self
	if pos then table.insert(self.Columns, pos, c) else self.Columns[#self.Columns + 1] = c end
	for i, col in ipairs(self.Columns) do col:SetColumnID(i) end
	self:SetDirty(true)
	return c
end
function DListView:ColumnWidth(i) local c = self.Columns[i]; return c and c.w or 0 end
function DListView:AddLine(...)
	local line = vgui.Create("DListView_Line", self.pnlCanvas)
	line:SetListView(self)
	local id = #self.Lines + 1
	line:SetID(id)
	self.Lines[id] = line
	self.Sorted[#self.Sorted + 1] = line
	for i = 1, select("#", ...) do line:SetColumnText(i, (select(i, ...))) end
	self:SetDirty(true)
	return line
end
function DListView:GetLine(i) return self.Lines[i] end
function DListView:GetLines() return self.Lines end
function DListView:RemoveLine(i)
	local l = self.Lines[i]
	if not l then return end
	l:Remove()
	self.Lines[i] = nil
	for k = #self.Sorted, 1, -1 do if self.Sorted[k] == l then table.remove(self.Sorted, k) end end
	self:SetDirty(true)
end
function DListView:Clear()
	for _, l in pairs(self.Lines) do l:Remove() end
	self.Lines, self.Sorted = {}, {}
	self:SetDirty(true)
end
function DListView:GetSortedID(i) local l = self.Sorted[i]; return l and l:GetID() end
function DListView:ClearSelection() for _, l in pairs(self.Lines) do l:SetSelected(false) end end
function DListView:GetSelected() local out = {}; for _, l in ipairs(self.Sorted) do if l:IsLineSelected() then out[#out + 1] = l end end return out end
function DListView:GetSelectedLine() for _, l in ipairs(self.Sorted) do if l:IsLineSelected() then return l:GetID(), l end end end
function DListView:SelectItem(l) if not l then return end self:ClearSelection(); l:SetSelected(true); self:OnRowSelected(l:GetID(), l); l:OnSelect() end
function DListView:SelectFirstItem() self:SelectItem(self.Sorted[1]) end
function DListView:OnClickLine(line, clear)
	if self.__multi and (input and input.IsKeyDown and input.IsKeyDown(KEY_LCONTROL)) then line:SetSelected(not line:IsLineSelected())
	else self:ClearSelection(); line:SetSelected(true) end
	self:OnRowSelected(line:GetID(), line)
	line:OnSelect()
end
function DListView:OnRowSelected(id, line) end
function DListView:OnRowRightClick(id, line) end
function DListView:DoDoubleClick(id, line) end
function DListView:OnRequestResize(col, w) col:SetWidth(w) end
function DListView:SortByColumn(i, desc)
	table.sort(self.Sorted, function(a, b)
		local av, bv = a:GetSortValue(i) or a:GetColumnText(i), b:GetSortValue(i) or b:GetColumnText(i)
		local an, bn = tonumber(av), tonumber(bv)
		if an and bn then av, bv = an, bn else av, bv = tostring(av):lower(), tostring(bv):lower() end
		if desc then return av > bv end
		return av < bv
	end)
	self:SetDirty(true)
end
function DListView:SortByColumns(c1, d1, c2, d2) if c1 then self:SortByColumn(c1, d1) end end
function DListView:FixColumnsLayout()
	local n = #self.Columns
	if n == 0 then return end
	local total = self.w - (self.VBar.visible and self.VBar.w or 0)
	local fixed, flex = 0, 0
	for _, c in ipairs(self.Columns) do if c.__fixed_w then fixed = fixed + c.__fixed_w else flex = flex + 1 end end
	local each = flex > 0 and math.floor((total - fixed) / flex) or 0
	local x = 0
	for _, c in ipairs(self.Columns) do
		local w = c.__fixed_w or math.max(c.__min_w, math.min(c.__max_w, each))
		c:SetPos(x, 0); c:SetSize(w, self.__header_h)
		c:SetVisible(not self.__hide_headers)
		x = x + w
	end
end
function DListView:DataLayout()
	local y, alt = 0, false
	for _, l in ipairs(self.Sorted) do
		l:SetPos(0, y); l:SetSize(self.pnlCanvas.w, self.__data_h)
		l:SetAltLine(alt); alt = not alt
		l:DataLayout(self)
		y = y + self.__data_h
	end
	return y
end
function DListView:PerformLayout(w, h)
	local hh = self.__hide_headers and 0 or self.__header_h
	local inner = h - hh
	self:FixColumnsLayout()
	local data_h = #self.Sorted * self.__data_h
	local bar = not self.__no_bar and data_h > inner
	self.VBar:SetVisible(bar); self.VBar:SetEnabled(bar)
	self.VBar:SetPos(w - 15, hh); self.VBar:SetSize(15, inner)
	self.VBar:SetUp(inner, data_h)
	self.pnlCanvas:SetPos(0, hh + (bar and self.VBar:GetOffset() or 0))
	self.pnlCanvas:SetSize(bar and (w - 15) or w, math.max(data_h, inner))
	self:DataLayout()
	self.__dirty = false
end
function DListView:OnVScroll(off) self.pnlCanvas:SetPos(0, (self.__hide_headers and 0 or self.__header_h) + off) end
function DListView:OnMouseWheeled(d) if self.VBar.visible then return self.VBar:OnMouseWheeled(d) end return false end
function DListView:Paint(w, h) surface.SetDrawColor(self.__bg_color or Color(255, 255, 255)); surface.DrawRect(0, 0, w, h); surface.SetDrawColor(SKIN.colButtonBorder); surface.DrawOutlinedRect(0, 0, w, h) end

-- DListBox: a one-column list
local DListBox = derma.DefineControl("DListBox", "List box", {}, "DListView")
function DListBox:Init() self:SetHideHeaders(true); self:AddColumn(""); self.__multiple = false end
function DListBox:SetMultiple(b) self.__multiple = b == true; self:SetMultiSelect(b) end
function DListBox:GetMultiple() return self.__multiple end
function DListBox:AddItem(text) local l = self:AddLine(text); return l end
function DListBox:SelectByName(name) for _, l in ipairs(self.Sorted) do if l:GetColumnText(1) == name then self:SelectItem(l) return end end end
function DListBox:SelectItem(item) DListView.SelectItem(self, item) end
function DListBox:GetSelectedItems() return self:GetSelected() end
function DListBox:GetSelectedValues() local out = {}; for _, l in ipairs(self:GetSelected()) do out[#out + 1] = l:GetColumnText(1) end return out end
function DListBox:SetSelectedItems(items) self:ClearSelection(); for _, l in ipairs(items) do l:SetSelected(true) end end

-- DPropertySheet / DTab: tabs along the top, one active page
local DTab = derma.DefineControl("DTab", "Property sheet tab", {}, "DButton")
function DTab:Init() self.__align = 5; self.__paint_bg = false; self:SetTall(20) end
function DTab:Setup(label, sheet, panel, icon) self:SetText(label); self.__sheet, self.__panel, self.__icon = sheet, panel, icon; self:SizeToContents(); self:SetWide(self.w + 10) end
function DTab:GetPanel() return self.__panel end
function DTab:SetPanel(p) self.__panel = p end
function DTab:GetPropertySheet() return self.__sheet end
function DTab:SetPropertySheet(s) self.__sheet = s end
function DTab:IsActive() return self.__sheet and self.__sheet:GetActiveTab() == self end
function DTab:GetTabHeight() return self:IsActive() and 28 or 20 end
function DTab:DoClick() if self.__sheet then self.__sheet:SetActiveTab(self) end end
function DTab:Paint(w, h)
	surface.SetDrawColor(self:IsActive() and SKIN.colTab or SKIN.colTabInactive)
	surface.DrawRect(0, 0, w, h)
	self:__draw_text(w, h, self:IsActive() and SKIN.colTabText or SKIN.colTabTextInactive)
end

local DPropertySheet = derma.DefineControl("DPropertySheet", "Tabbed pages", {}, "DPanel")
function DPropertySheet:Init()
	self.Items, self.__active = {}, nil
	self.__sheet_pad, self.__fade, self.__icons = 8, 0.1, true
	self.tabScroller = vgui.Create("Panel", self)
	self.tabScroller.mouse_input = true
	self.__paint_bg = true
end
function DPropertySheet:AddSheet(label, panel, icon, noStretchX, noStretchY, tooltip)
	if not panel or not panel.IsValid then return end
	local tab = vgui.Create("DTab", self.tabScroller)
	tab:Setup(label, self, panel, icon)
	tab:SetTooltip(tooltip)
	panel:SetParent(self)
	panel:SetVisible(false)
	local sheet = { Name = label, Tab = tab, Panel = panel }
	self.Items[#self.Items + 1] = sheet
	if not self.__active then self:SetActiveTab(tab) end
	self:InvalidateLayout()
	return sheet
end
function DPropertySheet:GetItems() return self.Items end
function DPropertySheet:GetActiveTab() return self.__active end
function DPropertySheet:SetActiveTab(tab)
	if self.__active == tab then return end
	local old = self.__active
	if old and old.__valid and old:GetPanel() then old:GetPanel():SetVisible(false) end
	self.__active = tab
	if tab and tab:GetPanel() then tab:GetPanel():SetVisible(true) end
	self:OnActiveTabChanged(old, tab)
	self:InvalidateLayout()
end
function DPropertySheet:OnActiveTabChanged(old, new) end
function DPropertySheet:SwitchToName(name) for _, s in ipairs(self.Items) do if s.Name == name then self:SetActiveTab(s.Tab) return true end end return false end
function DPropertySheet:CloseTab(tab, remove)
	for i, s in ipairs(self.Items) do
		if s.Tab == tab then
			table.remove(self.Items, i)
			if self.__active == tab then self.__active = nil; if self.Items[1] then self:SetActiveTab(self.Items[1].Tab) end end
			tab:Remove()
			if remove then s.Panel:Remove() else s.Panel:SetParent(nil) end
			self:InvalidateLayout()
			return s.Panel
		end
	end
end
function DPropertySheet:SetPadding(p) self.__sheet_pad = p end
function DPropertySheet:GetPadding() return self.__sheet_pad end
function DPropertySheet:SetFadeTime(t) self.__fade = t end
function DPropertySheet:GetFadeTime() return self.__fade end
function DPropertySheet:SetShowIcons(b) self.__icons = b ~= false end
function DPropertySheet:GetShowIcons() return self.__icons end
function DPropertySheet:CrossFade() end
function DPropertySheet:SetupCloseButton(fn) self.__close_fn = fn end
function DPropertySheet:SizeToContentWidth() local w = 0; for _, s in ipairs(self.Items) do w = w + s.Tab.w end self:SetWide(w + self.__sheet_pad * 2) end
function DPropertySheet:PerformLayout(w, h)
	self.tabScroller:SetPos(3, 0); self.tabScroller:SetSize(w - 6, 28)
	local x = 0
	for _, s in ipairs(self.Items) do
		local th = s.Tab:GetTabHeight()
		s.Tab:SetPos(x, 28 - th); s.Tab:SetTall(th)
		x = x + s.Tab.w
		s.Panel:SetVisible(s.Tab == self.__active)
		s.Panel:SetPos(self.__sheet_pad, 28 + self.__sheet_pad)
		s.Panel:SetSize(w - self.__sheet_pad * 2, h - 28 - self.__sheet_pad * 2)
	end
end
function DPropertySheet:Paint(w, h)
	surface.SetDrawColor(SKIN.colPropertySheet)
	surface.DrawRect(0, 20, w, h - 20)
	surface.SetDrawColor(SKIN.colTabShadow)
	surface.DrawOutlinedRect(0, 20, w, h - 20)
end

-- DCollapsibleCategory / DCategoryList / DForm
local DCollapsibleCategory = derma.DefineControl("DCollapsibleCategory", "Collapsible category", {}, "Panel")
function DCollapsibleCategory:Init()
	self.__expanded, self.__header_h, self.__anim, self.__cat_pad = true, 20, 0.2, 10
	self.Header = vgui.Create("DButton", self)
	self.Header:SetText("Category")
	self.Header.__align = 4
	self.Header.__inset = { 4, 0 }
	self.Header.__paint_bg = false
	self.Header.DoClick = function() self:Toggle() end
	self.__paint_bg = true
	self.mouse_input = true
	self.__list = nil
end
function DCollapsibleCategory:SetLabel(t) self.Header:SetText(t) end
function DCollapsibleCategory:SetExpanded(b) self.__expanded = b ~= false; self:InvalidateLayout(); self:InvalidateParent() end
function DCollapsibleCategory:GetExpanded() return self.__expanded end
function DCollapsibleCategory:Toggle() self:SetExpanded(not self.__expanded); self:OnToggle(self.__expanded) end
function DCollapsibleCategory:DoExpansion(b) self:SetExpanded(b) end
function DCollapsibleCategory:OnToggle(b) end
function DCollapsibleCategory:SetHeaderHeight(h) self.__header_h = h end
function DCollapsibleCategory:GetHeaderHeight() return self.__header_h end
function DCollapsibleCategory:SetAnimTime(t) self.__anim = t end
function DCollapsibleCategory:GetAnimTime() return self.__anim end
function DCollapsibleCategory:SetPadding(p) self.__cat_pad = p end
function DCollapsibleCategory:GetPadding() return self.__cat_pad end
function DCollapsibleCategory:SetStartHeight(h) self.__start_h = h end
function DCollapsibleCategory:GetStartHeight() return self.__start_h end
function DCollapsibleCategory:SetDrawBackground(b) self.__paint_bg = b ~= false end
function DCollapsibleCategory:GetDrawBackground() return self.__paint_bg ~= false end
function DCollapsibleCategory:SetPaintBackground(b) self.__paint_bg = b ~= false end
function DCollapsibleCategory:GetPaintBackground() return self.__paint_bg ~= false end
function DCollapsibleCategory:SetList(l) self.__list = l end
function DCollapsibleCategory:GetList() return self.__list end
function DCollapsibleCategory:SetContents(p) self.Contents = p; p:SetParent(self); self:InvalidateLayout() end
function DCollapsibleCategory:Add(class)
	if not self.Contents then local c = vgui.Create("DPanel", self); c.__paint_bg = false; self.Contents = c end
	local p = type(class) == "string" and vgui.Create(class, self.Contents) or class
	if type(class) ~= "string" then p:SetParent(self.Contents) end
	p:Dock(TOP)
	self:InvalidateLayout()
	return p
end
function DCollapsibleCategory:AnimSlide() end
function DCollapsibleCategory:UpdateAltLines() end
function DCollapsibleCategory:PerformLayout(w, h)
	self.Header:SetPos(0, 0); self.Header:SetSize(w, self.__header_h)
	if not self.Contents then self:SetTall(self.__header_h) return end
	self.Contents:SetVisible(self.__expanded)
	if self.__expanded then
		self.Contents:SetPos(0, self.__header_h)
		self.Contents:SetWide(w)
		self.Contents:__layout()
		local _, ch = self.Contents:ChildrenSize()
		self.Contents:SetTall(ch)
		self:SetTall(self.__header_h + ch)
	else
		self:SetTall(self.__header_h)
	end
end
function DCollapsibleCategory:Paint(w, h)
	if not self.__paint_bg then return end
	surface.SetDrawColor(SKIN.colCollapsibleCategory)
	surface.DrawRect(0, 0, w, h)
	surface.SetDrawColor(SKIN.bg_color_dark)
	surface.DrawRect(0, 0, w, self.__header_h)
end

local DCategoryList = derma.DefineControl("DCategoryList", "Category list", {}, "DScrollPanel")
function DCategoryList:Add(name)
	local c = vgui.Create("DCollapsibleCategory", self.pnlCanvas)
	c:SetLabel(name)
	c:SetList(self)
	c:Dock(TOP)
	c:DockMargin(0, 0, 0, 2)
	return c
end
function DCategoryList:AddItem(p) p:SetParent(self.pnlCanvas); p:Dock(TOP) end
function DCategoryList:UnselectAll() for _, c in ipairs(self.pnlCanvas.children) do if c.UnselectAll then c:UnselectAll() end end end

local DForm = derma.DefineControl("DForm", "Form", {}, "DCollapsibleCategory")
function DForm:Init()
	DCollapsibleCategory.Init(self)
	self.Items = {}
	self.__spacing, self.__autosize = 4, true
	self:SetLabel("Form")
	local c = vgui.Create("DPanel", self); c.__paint_bg = false; c:DockPadding(10, 4, 10, 4); self.Contents = c
end
function DForm:SetName(n) self:SetLabel(n) end
function DForm:SetSpacing(s) self.__spacing = s end
function DForm:GetSpacing() return self.__spacing end
function DForm:SetAutoSize(b) self.__autosize = b ~= false end
function DForm:GetAutoSize() return self.__autosize end
function DForm:AddItem(left, right)
	local row = vgui.Create("DPanel", self.Contents)
	row.__paint_bg = false
	row:Dock(TOP)
	row:DockMargin(0, 0, 0, self.__spacing)
	if right then
		left:SetParent(row); left:Dock(LEFT); left:SetWide(100)
		right:SetParent(row); right:Dock(FILL)
		row:SetTall(math.max(left.h, right.h))
	else
		left:SetParent(row); left:Dock(FILL)
		row:SetTall(left.h)
	end
	self.Items[#self.Items + 1] = row
	self:InvalidateLayout()
	return row
end
function DForm:Rebuild() self:InvalidateLayout(true) end
function DForm:Help(text) local l = vgui.Create("DLabel"); l:SetText(text); l:SetDark(true); l:SetWrap(true); l:SetAutoStretchVertical(true); l:SizeToContents(); self:AddItem(l); return l end
function DForm:ControlHelp(text) local l = self:Help(text); l.__text_color = Color(0, 0, 0, 170); return l end
function DForm:Button(text, cmd, ...) local b = vgui.Create("DButton"); b:SetText(text); if cmd then local args = { ... }; b.DoClick = function() RunConsoleCommand(cmd, table.unpack(args)) end end self:AddItem(b); return b end
function DForm:CheckBox(label, cvar) local c = vgui.Create("DCheckBoxLabel"); c:SetText(label); c:SetDark(true); if cvar then c:SetConVar(cvar); local cv = GetConVar(cvar); if cv then c:SetChecked(cv:GetBool()) end end c:SizeToContents(); self:AddItem(c); return c end
function DForm:NumSlider(label, cvar, min, max, decimals) local s = vgui.Create("DNumSlider"); s:SetText(label); s:SetDark(true); s:SetMinMax(min or 0, max or 1); s:SetDecimals(decimals or 2); if cvar then s:SetConVar(cvar) end self:AddItem(s); return s end
function DForm:TextEntry(label, cvar) local l = vgui.Create("DLabel"); l:SetText(label); l:SetDark(true); local e = vgui.Create("DTextEntry"); if cvar then e:SetConVar(cvar); e:UpdateConvarValue() end self:AddItem(l, e); return e, l end
function DForm:ComboBox(label, cvar) local l = vgui.Create("DLabel"); l:SetText(label); l:SetDark(true); local c = vgui.Create("DComboBox"); if cvar then c:SetConVar(cvar) end self:AddItem(l, c); return c, l end
function DForm:NumberWang(label, cvar, min, max, decimals) local l = vgui.Create("DLabel"); l:SetText(label); l:SetDark(true); local w = vgui.Create("DNumberWang"); w:SetMinMax(min or 0, max or 100); w:SetDecimals(decimals or 0); if cvar then w:SetConVar(cvar) end self:AddItem(l, w); return w, l end
function DForm:ListBox(label) local l = vgui.Create("DLabel"); l:SetText(label); l:SetDark(true); local b = vgui.Create("DListBox"); b:SetTall(100); self:AddItem(l, b); return b, l end
function DForm:PanelSelect() local p = vgui.Create("DPanelSelect"); self:AddItem(p); return p end
function DForm:PropSelect(label, cvar, models, h) local p = vgui.Create("PropSelect"); p:SetTall(h or 100); self:AddItem(p); return p end

-- DNumberWang: a numeric text entry with up / down buttons
local DNumberWang = derma.DefineControl("DNumberWang", "Number entry", {}, "DTextEntry")
function DNumberWang:Init()
	DTextEntry.Init(self)
	self:SetNumeric(true)
	self.__min, self.__max, self.__decimals, self.__interval = 0, 100, 0, 1
	self.__wang_hidden = false
	self:SetSize(64, 20)
	self.Up = vgui.Create("DButton", self); self.Up:SetText(""); self.Up.DoClick = function() self:SetValue(self:GetValue() + self.__interval) end
	self.Down = vgui.Create("DButton", self); self.Down:SetText(""); self.Down.DoClick = function() self:SetValue(self:GetValue() - self.__interval) end
	self:SetValue(0)
end
function DNumberWang:SetMin(m) self.__min = m end
function DNumberWang:SetMax(m) self.__max = m end
function DNumberWang:GetMin() return self.__min end
function DNumberWang:GetMax() return self.__max end
function DNumberWang:SetMinMax(a, b) self.__min, self.__max = a, b end
function DNumberWang:SetDecimals(d) self.__decimals = d or 0 end
function DNumberWang:GetDecimals() return self.__decimals end
function DNumberWang:SetInterval(i) self.__interval = i end
function DNumberWang:GetInterval() return self.__interval end
function DNumberWang:HideWang() self.__wang_hidden = true; self.Up:SetVisible(false); self.Down:SetVisible(false) end
function DNumberWang:GetTextArea() return self end
function DNumberWang:GetFloatValue() return tonumber(self.__text) or 0 end
function DNumberWang:SetFloatValue(v) self:SetValue(v) end
function DNumberWang:GetFraction(v) v = v or self:GetValue(); return (v - self.__min) / math.max(1e-9, self.__max - self.__min) end
function DNumberWang:SetFraction(f) self:SetValue(self.__min + (self.__max - self.__min) * f) end
function DNumberWang:GetValue() return tonumber(self.__text) or 0 end
function DNumberWang:SetValue(v)
	v = tonumber(v) or 0
	v = math.max(self.__min, math.min(self.__max, v))
	local p = 10 ^ self.__decimals
	v = math.floor(v * p + 0.5) / p
	local text = self.__decimals > 0 and string.format("%." .. self.__decimals .. "f", v) or tostring(math.floor(v))
	if text == self.__text then return end
	self:SetText(text)
	self:OnValueChanged(v)
	self:ConVarChanged(v)
end
function DNumberWang:OnValueChanged(v) end
function DNumberWang:OnValueChange(v) self:SetValue(v) end
function DNumberWang:OnEnter() self:SetValue(self.__text) end
function DNumberWang:PerformLayout(w, h)
	if self.__wang_hidden then return end
	self.Up:SetPos(w - 12, 0); self.Up:SetSize(12, h / 2)
	self.Down:SetPos(w - 12, h / 2); self.Down:SetSize(12, h / 2)
end

-- DLabelEditable: a label that becomes a text entry on double click
local DLabelEditable = derma.DefineControl("DLabelEditable", "Editable label", {}, "DLabel")
function DLabelEditable:Init() self.mouse_input = true; self.__auto_stretch = true end
function DLabelEditable:SetAutoStretch(b) self.__auto_stretch = b == true end
function DLabelEditable:GetAutoStretch() return self.__auto_stretch end
function DLabelEditable:IsEditing() return self.__edit ~= nil and self.__edit.__valid end
function DLabelEditable:OnLabelTextChanged(text) return text end
function DLabelEditable:DoDoubleClick()
	local e = vgui.Create("DTextEntry", self)
	e:SetText(self:GetText())
	e:SetSize(self.w, self.h)
	e:RequestFocus()
	e:SelectAllText()
	e.OnEnter = function() self:SetText(self:OnLabelTextChanged(e:GetValue()) or e:GetValue()); e:Remove(); self.__edit = nil end
	e.OnLoseFocus = function() if e.__valid then e.OnEnter() end end
	self.__edit = e
end
function DLabelEditable:OnMousePressed(code)
	local now = SysTime()
	if self.__last and now - self.__last < 0.3 then self:DoDoubleClick(); self.__last = nil return end
	self.__last = now
	DLabel.OnMousePressed(self, code)
end

-- dividers
local DHorizontalDivider = derma.DefineControl("DHorizontalDivider", "Two panes side by side", {}, "DPanel")
function DHorizontalDivider:Init() self.__paint_bg = false; self.__div_w, self.__left_w, self.__left_min, self.__right_min = 8, 100, 50, 50; self.__middle = nil; self.mouse_input = true end
function DHorizontalDivider:SetLeft(p) self.__left = p; p:SetParent(self) end
function DHorizontalDivider:GetLeft() return self.__left end
function DHorizontalDivider:SetRight(p) self.__right = p; p:SetParent(self) end
function DHorizontalDivider:GetRight() return self.__right end
function DHorizontalDivider:SetMiddle(p) self.__middle = p; p:SetParent(self) end
function DHorizontalDivider:GetMiddle() return self.__middle end
function DHorizontalDivider:SetDividerWidth(w) self.__div_w = w end
function DHorizontalDivider:GetDividerWidth() return self.__div_w end
function DHorizontalDivider:SetLeftWidth(w) self.__left_w = w; self:InvalidateLayout() end
function DHorizontalDivider:GetLeftWidth() return self.__left_w end
function DHorizontalDivider:SetLeftMin(m) self.__left_min = m end
function DHorizontalDivider:GetLeftMin() return self.__left_min end
function DHorizontalDivider:SetRightMin(m) self.__right_min = m end
function DHorizontalDivider:GetRightMin() return self.__right_min end
function DHorizontalDivider:SetDragging(b) self.__dragging = b end
function DHorizontalDivider:GetDragging() return self.__dragging == true end
function DHorizontalDivider:SetHoldPos(p) self.__hold = p end
function DHorizontalDivider:GetHoldPos() return self.__hold end
function DHorizontalDivider:StartGrab() self.__dragging = true; self.__hold = (self:CursorPos()) - self.__left_w; self:MouseCapture(true) end
function DHorizontalDivider:OnMousePressed() local lx = self:CursorPos(); if lx >= self.__left_w and lx <= self.__left_w + self.__div_w then self:StartGrab() end end
function DHorizontalDivider:OnMouseReleased() self.__dragging = false; self:MouseCapture(false) end
function DHorizontalDivider:Think()
	if not self.__dragging then return end
	local lx = self:CursorPos()
	self:SetLeftWidth(math.max(self.__left_min, math.min(self.w - self.__right_min - self.__div_w, lx - (self.__hold or 0))))
end
function DHorizontalDivider:PerformLayout(w, h)
	if self.__left then self.__left:SetPos(0, 0); self.__left:SetSize(self.__left_w, h) end
	if self.__middle then self.__middle:SetPos(self.__left_w, 0); self.__middle:SetSize(self.__div_w, h) end
	if self.__right then self.__right:SetPos(self.__left_w + self.__div_w, 0); self.__right:SetSize(w - self.__left_w - self.__div_w, h) end
end

local DVerticalDivider = derma.DefineControl("DVerticalDivider", "Two panes stacked", {}, "DPanel")
function DVerticalDivider:Init() self.__paint_bg = false; self.__div_h, self.__top_h, self.__top_min, self.__bottom_min, self.__top_max = 8, 100, 50, 50, nil; self.mouse_input = true end
function DVerticalDivider:SetTop(p) self.__top = p; p:SetParent(self) end
function DVerticalDivider:GetTop() return self.__top end
function DVerticalDivider:SetBottom(p) self.__bottom = p; p:SetParent(self) end
function DVerticalDivider:GetBottom() return self.__bottom end
function DVerticalDivider:SetMiddle(p) self.__middle = p; p:SetParent(self) end
function DVerticalDivider:GetMiddle() return self.__middle end
function DVerticalDivider:SetDividerHeight(h) self.__div_h = h end
function DVerticalDivider:GetDividerHeight() return self.__div_h end
function DVerticalDivider:SetTopHeight(h) self.__top_h = h; self:InvalidateLayout() end
function DVerticalDivider:GetTopHeight() return self.__top_h end
function DVerticalDivider:SetTopMin(m) self.__top_min = m end
function DVerticalDivider:GetTopMin() return self.__top_min end
function DVerticalDivider:SetTopMax(m) self.__top_max = m end
function DVerticalDivider:GetTopMax() return self.__top_max end
function DVerticalDivider:SetBottomMin(m) self.__bottom_min = m end
function DVerticalDivider:GetBottomMin() return self.__bottom_min end
function DVerticalDivider:SetDragging(b) self.__dragging = b end
function DVerticalDivider:GetDragging() return self.__dragging == true end
function DVerticalDivider:SetHoldPos(p) self.__hold = p end
function DVerticalDivider:GetHoldPos() return self.__hold end
function DVerticalDivider:StartGrab() self.__dragging = true; self.__hold = select(2, self:CursorPos()) - self.__top_h; self:MouseCapture(true) end
function DVerticalDivider:DoConstraints() self.__top_h = math.max(self.__top_min, math.min(self.h - self.__bottom_min - self.__div_h, self.__top_max and math.min(self.__top_h, self.__top_max) or self.__top_h)) end
function DVerticalDivider:OnMousePressed() local _, ly = self:CursorPos(); if ly >= self.__top_h and ly <= self.__top_h + self.__div_h then self:StartGrab() end end
function DVerticalDivider:OnMouseReleased() self.__dragging = false; self:MouseCapture(false) end
function DVerticalDivider:Think() if not self.__dragging then return end local _, ly = self:CursorPos(); self.__top_h = ly - (self.__hold or 0); self:DoConstraints(); self:InvalidateLayout() end
function DVerticalDivider:PerformLayout(w, h)
	self:DoConstraints()
	if self.__top then self.__top:SetPos(0, 0); self.__top:SetSize(w, self.__top_h) end
	if self.__middle then self.__middle:SetPos(0, self.__top_h); self.__middle:SetSize(w, self.__div_h) end
	if self.__bottom then self.__bottom:SetPos(0, self.__top_h + self.__div_h); self.__bottom:SetSize(w, h - self.__top_h - self.__div_h) end
end

-- DImageButton
local DImageButton = derma.DefineControl("DImageButton", "Image button", {}, "DButton")
function DImageButton:Init() self.__paint_bg = false; self:SetText(""); self.__stretch, self.__keep_aspect, self.__img_visible = true, false, true; self.__color = Color(255, 255, 255) end
function DImageButton:SetImage(path) self.__icon = Material(path); self.__icon_path = path end
function DImageButton:GetImage() return self.__icon_path or "" end
function DImageButton:SetIcon(path) self:SetImage(path) end
function DImageButton:SetMaterial(m) self.__icon = m end
function DImageButton:SetOnViewMaterial(m) self:SetImage(m) end
function DImageButton:SetDepressImage(b) self.__depress = b ~= false end
function DImageButton:GetDepressImage() return self.__depress ~= false end
function DImageButton:DepressImage() end
function DImageButton:SetStretchToFit(b) self.__stretch = b ~= false end
function DImageButton:GetStretchToFit() return self.__stretch end
function DImageButton:SetKeepAspect(b) self.__keep_aspect = b == true end
function DImageButton:SetImageVisible(b) self.__img_visible = b ~= false end
function DImageButton:SetColor(c) self.__color = c end
function DImageButton:Paint(w, h)
	if not self.__img_visible or not self.__icon then return end
	surface.SetMaterial(self.__icon)
	surface.SetDrawColor(self.__color)
	if self.__down and self.__depress ~= false then surface.DrawTexturedRect(2, 2, w - 4, h - 4) else surface.DrawTexturedRect(0, 0, w, h) end
end

-- DMenuBar
local DMenuBar = derma.DefineControl("DMenuBar", "Menu bar", {}, "DPanel")
function DMenuBar:Init() self.Menus = {}; self:SetTall(24); self.__paint_bg = true; self.__is_menu = false end
function DMenuBar:AddMenu(label)
	local m = vgui.Create("DMenu", self)
	m:SetDeleteSelf(false)
	m:SetVisible(false)
	m:SetParent(world_panel)
	local b = vgui.Create("DButton", self)
	b:SetText(label)
	b.__paint_bg = false
	b:Dock(LEFT)
	b:SizeToContents()
	b:SetWide(b.w + 10)
	b.DoClick = function() if m.visible then m:Hide() else local x, y = b:LocalToScreen(0, b.h); m:Open(x, y) end end
	self.Menus[label] = m
	return m
end
function DMenuBar:AddOrGetMenu(label) return self.Menus[label] or self:AddMenu(label) end
function DMenuBar:GetOpenMenu() for _, m in pairs(self.Menus) do if m.visible then return m end end end
function DMenuBar:SetIsMenu(b) self.__is_menu = b == true end
function DMenuBar:GetIsMenu() return self.__is_menu end
function DMenuBar:GetDrawBackground() return self.__paint_bg ~= false end
function DMenuBar:SetDrawBackground(b) self.__paint_bg = b ~= false end
function DMenuBar:GetPaintBackground() return self.__paint_bg ~= false end
function DMenuBar:SetPaintBackground(b) self.__paint_bg = b ~= false end
function DMenuBar:Paint(w, h) if self.__paint_bg then surface.SetDrawColor(SKIN.bg_color); surface.DrawRect(0, 0, w, h) end end

-- DTooltip: a floating label above the hovered panel
local DTooltip = derma.DefineControl("DTooltip", "Tooltip", {}, "DLabel")
function DTooltip:Init() self.mouse_input = false; self.__text_color = SKIN.Colours.Tooltip.Text; self:SetParent(world_panel) end
function DTooltip:SetContents(p, del) self.__contents = p; p:SetParent(self) end
function DTooltip:OpenForPanel(p) self.__target = p; self:SetText(p:GetTooltip() or ""); self:SizeToContents(); self:SetSize(self.w + 8, self.h + 6); self:PositionTooltip() end
function DTooltip:PositionTooltip() if not self.__target then return end local x, y = self.__target:LocalToScreen(self.__target.w / 2, 0); self:SetPos(math.floor(x - self.w / 2), y - self.h - 8) end
function DTooltip:DrawArrow() end
function DTooltip:Close() self:Remove() end
function DTooltip:Paint(w, h) surface.SetDrawColor(SKIN.Colours.Tooltip.Background); surface.DrawRect(0, 0, w, h); surface.SetDrawColor(SKIN.colButtonBorder); surface.DrawOutlinedRect(0, 0, w, h); self.__align = 5; self:__draw_text(w, h, SKIN.Colours.Tooltip.Text) end
local tooltip
local function tooltip_think()
	local target = hovered
	while target and not target.__tooltip do target = target.parent end
	if target and (not tooltip or not tooltip.__valid or tooltip.__target ~= target) then
		if tooltip and tooltip.__valid then tooltip:Remove() end
		tooltip = vgui.Create("DTooltip")
		tooltip:OpenForPanel(target)
	elseif not target and tooltip and tooltip.__valid then
		tooltip:Remove(); tooltip = nil
	elseif tooltip and tooltip.__valid then
		tooltip:PositionTooltip()
	end
end
if events then events.on("render.hud", function() tooltip_think() end) end

-- DPanelList (legacy list of panels), DIconLayout, DGrid, DTileLayout
local DPanelList = derma.DefineControl("DPanelList", "Panel list", {}, "DScrollPanel")
function DPanelList:Init() DScrollPanel.Init(self); self.Items = {}; self.__list_pad, self.__spacing, self.__autosize = 0, 0, false end
function DPanelList:SetPadding(p) self.__list_pad = p; self:InvalidateLayout() end
function DPanelList:GetPadding() return self.__list_pad end
function DPanelList:SetSpacing(s) self.__spacing = s; self:InvalidateLayout() end
function DPanelList:GetSpacing() return self.__spacing end
function DPanelList:SetAutoSize(b) self.__autosize = b == true end
function DPanelList:EnableVerticalScrollbar(b) self.__vbar = b end
function DPanelList:EnableHorizontal(b) end
function DPanelList:GetItems() return self.Items end
function DPanelList:AddItem(p, state) p:SetParent(self.pnlCanvas); self.Items[#self.Items + 1] = p; self:InvalidateLayout(); return p end
function DPanelList:InsertAtTop(p) p:SetParent(self.pnlCanvas); table.insert(self.Items, 1, p); self:InvalidateLayout() end
function DPanelList:RemoveItem(p) for i = #self.Items, 1, -1 do if self.Items[i] == p then table.remove(self.Items, i) end end p:Remove(); self:InvalidateLayout() end
function DPanelList:CleanList() for i = #self.Items, 1, -1 do if not self.Items[i].__valid then table.remove(self.Items, i) end end end
function DPanelList:Clear() for _, p in ipairs(self.Items) do p:Remove() end self.Items = {} end
function DPanelList:Rebuild() self:InvalidateLayout() end
function DPanelList:PerformLayout(w, h)
	self:CleanList()
	local y = self.__list_pad
	for _, p in ipairs(self.Items) do
		p:SetPos(self.__list_pad, y)
		p:SetWide(self.pnlCanvas.w - self.__list_pad * 2)
		y = y + p.h + self.__spacing
	end
	if self.__autosize then self:SetTall(y + self.__list_pad) end
	DScrollPanel.PerformLayout(self, w, h)
end

local DIconLayout = derma.DefineControl("DIconLayout", "Icon layout", {}, "DPanel")
function DIconLayout:Init() self.__paint_bg = false; self.__space_x, self.__space_y, self.__border, self.__dir = 0, 0, 0, TOP; self.__stretch_w, self.__stretch_h = false, false end
function DIconLayout:SetSpaceX(s) self.__space_x = s end
function DIconLayout:GetSpaceX() return self.__space_x end
function DIconLayout:SetSpaceY(s) self.__space_y = s end
function DIconLayout:GetSpaceY() return self.__space_y end
function DIconLayout:SetBorder(b) self.__border = b end
function DIconLayout:GetBorder() return self.__border end
function DIconLayout:SetLayoutDir(d) self.__dir = d end
function DIconLayout:GetLayoutDir() return self.__dir end
function DIconLayout:SetStretchWidth(b) self.__stretch_w = b == true end
function DIconLayout:GetStretchWidth() return self.__stretch_w end
function DIconLayout:SetStretchHeight(b) self.__stretch_h = b == true end
function DIconLayout:GetStretchHeight() return self.__stretch_h end
function DIconLayout:OnModified() end
function DIconLayout:Layout() self:InvalidateLayout(true) end
function DIconLayout:Copy() return nil end
function DIconLayout:CopyContents() end
function DIconLayout:LayoutIcons_TOP(w)
	local x, y, row_h = self.__border, self.__border, 0
	for _, c in ipairs(self:__ordered()) do
		if c.visible then
			if x + c.w > w - self.__border and x > self.__border then x = self.__border; y = y + row_h + self.__space_y; row_h = 0 end
			c:SetPos(x, y)
			x = x + c.w + self.__space_x
			row_h = math.max(row_h, c.h)
		end
	end
	return y + row_h + self.__border
end
function DIconLayout:LayoutIcons_LEFT(h)
	local x, y, col_w = self.__border, self.__border, 0
	for _, c in ipairs(self:__ordered()) do
		if c.visible then
			if y + c.h > h - self.__border and y > self.__border then y = self.__border; x = x + col_w + self.__space_x; col_w = 0 end
			c:SetPos(x, y)
			y = y + c.h + self.__space_y
			col_w = math.max(col_w, c.w)
		end
	end
	return x + col_w + self.__border
end
function DIconLayout:PerformLayout(w, h)
	if self.__dir == LEFT then local tw = self:LayoutIcons_LEFT(h); if self.__stretch_w then self:SetWide(tw) end
	else local th = self:LayoutIcons_TOP(w); if self.__stretch_h or self.__dir == TOP then self:SetTall(th) end end
end

local DGrid = derma.DefineControl("DGrid", "Grid", {}, "DPanel")
function DGrid:Init() self.__paint_bg = false; self.Items = {}; self.__cols, self.__col_w, self.__row_h = 1, 100, 100 end
function DGrid:SetCols(n) self.__cols = math.max(1, n or 1); self:InvalidateLayout() end
function DGrid:GetCols() return self.__cols end
function DGrid:SetColWide(w) self.__col_w = w; self:InvalidateLayout() end
function DGrid:GetColWide() return self.__col_w end
function DGrid:SetRowHeight(h) self.__row_h = h; self:InvalidateLayout() end
function DGrid:GetRowHeight() return self.__row_h end
function DGrid:GetItems() return self.Items end
function DGrid:AddItem(p) p:SetParent(self); self.Items[#self.Items + 1] = p; self:InvalidateLayout() end
function DGrid:RemoveItem(p, keep) for i = #self.Items, 1, -1 do if self.Items[i] == p then table.remove(self.Items, i) end end if not keep then p:Remove() end self:InvalidateLayout() end
function DGrid:SortByMember(key, desc) table.sort(self.Items, function(a, b) if desc == false then return (a[key] or 0) < (b[key] or 0) end return (a[key] or 0) > (b[key] or 0) end); self:InvalidateLayout() end
function DGrid:PerformLayout(w, h)
	for i, p in ipairs(self.Items) do
		local col, row = (i - 1) % self.__cols, math.floor((i - 1) / self.__cols)
		p:SetPos(col * self.__col_w, row * self.__row_h)
	end
	self:SetSize(self.__cols * self.__col_w, math.ceil(#self.Items / self.__cols) * self.__row_h)
end

local DTileLayout = derma.DefineControl("DTileLayout", "Tile layout", {}, "DIconLayout")
function DTileLayout:Init() DIconLayout.Init(self); self.__base, self.__min_h, self.__tiles = 32, 0, {} end
function DTileLayout:SetBaseSize(s) self.__base = s end
function DTileLayout:GetBaseSize() return self.__base end
function DTileLayout:SetMinHeight(h) self.__min_h = h end
function DTileLayout:GetMinHeight() return self.__min_h end
function DTileLayout:ClearTiles() self.__tiles = {} end
function DTileLayout:GetTile(x, y) return self.__tiles[y] and self.__tiles[y][x] end
function DTileLayout:SetTile(x, y, p) self.__tiles[y] = self.__tiles[y] or {}; self.__tiles[y][x] = p end
function DTileLayout:FitsInTile(x, y, w, h) for ty = y, y + h - 1 do for tx = x, x + w - 1 do if self:GetTile(tx, ty) then return false end end end return true end
function DTileLayout:ConsumeTiles(x, y, w, h, p) for ty = y, y + h - 1 do for tx = x, x + w - 1 do self:SetTile(tx, ty, p or true) end end end
function DTileLayout:FindFreeTile(x, y, w, h, cols) while true do if x + w > cols then x = 0; y = y + 1 end if self:FitsInTile(x, y, w, h) then return x, y end x = x + 1 end end
function DTileLayout:LayoutTiles(w)
	self:ClearTiles()
	local cols = math.max(1, math.floor((w - self.__border * 2) / (self.__base + self.__space_x)))
	local max_y = 0
	for _, c in ipairs(self:__ordered()) do
		if c.visible then
			local tw, th = math.max(1, math.ceil(c.w / (self.__base + self.__space_x))), math.max(1, math.ceil(c.h / (self.__base + self.__space_y)))
			local x, y = self:FindFreeTile(0, 0, tw, th, cols)
			self:ConsumeTiles(x, y, tw, th, c)
			c:SetPos(self.__border + x * (self.__base + self.__space_x), self.__border + y * (self.__base + self.__space_y))
			max_y = math.max(max_y, y + th)
		end
	end
	return math.max(self.__min_h, self.__border * 2 + max_y * (self.__base + self.__space_y))
end
function DTileLayout:Layout() self:InvalidateLayout(true) end
function DTileLayout:PerformLayout(w, h) self:SetTall(self:LayoutTiles(w)) end

-- DTree / DTree_Node: an expandable tree, one row per node
local DTree_Node = derma.DefineControl("DTree_Node", "Tree node", {}, "DPanel")
function DTree_Node:Init()
	self.__paint_bg = false
	self.__expanded, self.__nodes, self.__draw_lines = false, {}, true
	self.__double_click, self.__hide_expander, self.__force_expander = true, false, false
	self.Label = vgui.Create("DLabel", self)
	self.Label:SetText("Node")
	self.Label.mouse_input = true
	self.Label.OnMousePressed = function(_, code) self:InternalDoClick(code) end
	self.Expander = vgui.Create("DButton", self)
	self.Expander:SetText("")
	self.Expander:SetSize(15, 15)
	self.Expander.__paint_bg = false
	self.Expander.DoClick = function() self:SetExpanded(not self.__expanded) end
	self.Expander.Paint = function(e, w, h)
		if not self:HasChildren() and not self.__force_expander then return end
		surface.SetDrawColor(SKIN.Colours.Tree.Normal)
		surface.DrawOutlinedRect(2, 2, 11, 11)
		surface.DrawRect(4, 7, 7, 1)
		if not self.__expanded then surface.DrawRect(7, 4, 1, 7) end
	end
	self.ChildNodes = nil
	self:SetTall(17)
end
function DTree_Node:SetRoot(r) self.__root = r end
function DTree_Node:GetRoot() return self.__root end
function DTree_Node:IsRootNode() return self.__root == self end
function DTree_Node:SetParentNode(p) self.__parent_node = p end
function DTree_Node:GetParentNode() return self.__parent_node end
function DTree_Node:SetText(t) self.Label:SetText(t) end
function DTree_Node:GetText() return self.Label:GetText() end
function DTree_Node:SetIcon(i) self.__icon = i end
function DTree_Node:GetIcon() return self.__icon end
function DTree_Node:ShowIcons() return self.__root and self.__root:ShowIcons() or false end
function DTree_Node:GetLineHeight() return self.__root and self.__root:GetLineHeight() or 17 end
function DTree_Node:GetIndentSize() return self.__root and self.__root:GetIndentSize() or 14 end
function DTree_Node:SetDrawLines(b) self.__draw_lines = b ~= false end
function DTree_Node:GetDrawLines() return self.__draw_lines end
function DTree_Node:SetDoubleClickToOpen(b) self.__double_click = b ~= false end
function DTree_Node:GetDoubleClickToOpen() return self.__double_click end
function DTree_Node:SetHideExpander(b) self.__hide_expander = b == true end
function DTree_Node:GetHideExpander() return self.__hide_expander end
function DTree_Node:SetForceShowExpander(b) self.__force_expander = b == true end
function DTree_Node:GetForceShowExpander() return self.__force_expander end
function DTree_Node:SetLastChild(b) self.__last = b == true end
function DTree_Node:GetLastChild() return self.__last == true end
function DTree_Node:SetDirty(b) self.__dirty = b == true end
function DTree_Node:GetDirty() return self.__dirty == true end
function DTree_Node:SetDraggableName(n) self.__drag_name = n end
function DTree_Node:GetDraggableName() return self.__drag_name end
function DTree_Node:InstallDraggable(n) self.__drag_name = n end
function DTree_Node:SetNeedsPopulating(b) self.__needs_pop = b == true end
function DTree_Node:GetNeedsPopulating() return self.__needs_pop == true end
function DTree_Node:SetNeedsChildSearch(b) self.__needs_search = b == true end
function DTree_Node:GetNeedsChildSearch() return self.__needs_search == true end
function DTree_Node:SetFolder(f) self.__folder = f end
function DTree_Node:GetFolder() return self.__folder end
function DTree_Node:SetFileName(f) self.__file = f end
function DTree_Node:GetFileName() return self.__file end
function DTree_Node:SetPathID(p) self.__path_id = p end
function DTree_Node:GetPathID() return self.__path_id end
function DTree_Node:SetWildCard(w) self.__wildcard = w end
function DTree_Node:GetWildCard() return self.__wildcard end
function DTree_Node:SetShowFiles(b) self.__show_files = b == true end
function DTree_Node:GetShowFiles() return self.__show_files == true end
function DTree_Node:MakeFolder(folder, path, show_files, wildcard, dont_force) self.__folder, self.__path_id, self.__show_files, self.__wildcard = folder, path, show_files, wildcard; self:SetForceShowExpander(not dont_force); self.__needs_pop = true end
function DTree_Node:FilePopulate() end
function DTree_Node:FilePopulateCallback() end
function DTree_Node:PopulateChildren() if self.__needs_pop then self.__needs_pop = false; self:FilePopulate() end end
function DTree_Node:PopulateChildrenAndSelf() self:PopulateChildren() end
function DTree_Node:CreateChildNodes() if not self.ChildNodes then local c = vgui.Create("DPanel", self); c.__paint_bg = false; self.ChildNodes = c end end
function DTree_Node:GetChildNodes() return self.__nodes end
function DTree_Node:GetChildNode(i) return self.__nodes[i + 1] end
function DTree_Node:GetChildNodeCount() return #self.__nodes end
function DTree_Node:HasChildren() return #self.__nodes > 0 end
function DTree_Node:ChildExpanded() return self.__expanded end
function DTree_Node:AddNode(name, icon)
	self:CreateChildNodes()
	local n = vgui.Create("DTree_Node", self.ChildNodes)
	n:SetText(name)
	n:SetIcon(icon)
	n:SetRoot(self.__root or self)
	n:SetParentNode(self)
	self.__nodes[#self.__nodes + 1] = n
	self:OnNodeAdded(n)
	self:InvalidateLayout()
	return n
end
function DTree_Node:AddFolder(name, folder, path, show_files, wildcard, dont_force) local n = self:AddNode(name); n:MakeFolder(folder, path, show_files, wildcard, dont_force); return n end
function DTree_Node:AddPanel(p) self:CreateChildNodes(); p:SetParent(self.ChildNodes); self.__nodes[#self.__nodes + 1] = p; self:InvalidateLayout() end
function DTree_Node:InsertNode(n) self:CreateChildNodes(); n:SetParent(self.ChildNodes); n:SetParentNode(self); n:SetRoot(self.__root or self); self.__nodes[#self.__nodes + 1] = n; self:InvalidateLayout() end
function DTree_Node:Insert(n, before, after) self:InsertNode(n) end
function DTree_Node:MoveChildTo(n) self:InsertNode(n) end
function DTree_Node:MoveToTop() if self.__parent_node then local l = self.__parent_node.__nodes; for i = #l, 1, -1 do if l[i] == self then table.remove(l, i) end end table.insert(l, 1, self); self.__parent_node:InvalidateLayout() end end
function DTree_Node:OnNodeAdded(n) end
function DTree_Node:OnModified() end
function DTree_Node:OnNodeSelected(n) if self.__parent_node then self.__parent_node:OnNodeSelected(n) end end
function DTree_Node:DoChildrenOrder() end
function DTree_Node:CleanList() for i = #self.__nodes, 1, -1 do if not self.__nodes[i].__valid then table.remove(self.__nodes, i) end end end
function DTree_Node:SetExpanded(b, surpress)
	self.__expanded = b == true
	if b then self:PopulateChildren() end
	if self.ChildNodes then self.ChildNodes:SetVisible(self.__expanded) end
	self:InvalidateLayout()
	local p = self.__parent_node
	while p do p:InvalidateLayout(); p = p.__parent_node end
	if self.__root then self.__root:InvalidateLayout() end
end
function DTree_Node:GetExpanded() return self.__expanded end
function DTree_Node:ExpandRecurse(b) self:SetExpanded(b); for _, n in ipairs(self.__nodes) do if n.ExpandRecurse then n:ExpandRecurse(b) end end end
function DTree_Node:ExpandTo(b) local p = self.__parent_node; while p do p:SetExpanded(b); p = p.__parent_node end end
function DTree_Node:AnimSlide() end
function DTree_Node:SetSelected(b) self.__selected = b == true end
function DTree_Node:IsSelected() return self.__selected == true end
function DTree_Node:SetupCopy() end
function DTree_Node:Copy() return nil end
function DTree_Node:LeaveTree() end
function DTree_Node:DoClick() return false end
function DTree_Node:DoRightClick() return false end
function DTree_Node:InternalDoClick(code)
	if code == MOUSE_RIGHT then self:InternalDoRightClick() return end
	if self.__root then self.__root:SetSelectedItem(self) end
	if self:DoClick() then return end
	local now = SysTime()
	if self.__last_click and now - self.__last_click < 0.3 and self.__double_click then self:SetExpanded(not self.__expanded) end
	self.__last_click = now
end
function DTree_Node:InternalDoRightClick() if self.__root then self.__root:SetSelectedItem(self) end self:DoRightClick() end
function DTree_Node:PerformRootNodeLayout()
	local y = 0
	if self.ChildNodes then
		for _, n in ipairs(self.__nodes) do if n.visible then n:SetPos(0, y); n:SetWide(self.w); n:__layout(); y = y + n.h end end
		self.ChildNodes:SetPos(0, 0); self.ChildNodes:SetSize(self.w, y)
	end
	self:SetTall(y)
end
function DTree_Node:PerformLayout(w, h)
	if self:IsRootNode() then self:PerformRootNodeLayout() return end
	local lh, indent = self:GetLineHeight(), self:GetIndentSize()
	self.Expander:SetPos(0, (lh - 15) / 2)
	self.Expander:SetVisible(not self.__hide_expander)
	self.Label:SetPos(indent + (self:ShowIcons() and 18 or 2), 0)
	self.Label:SetSize(w - indent - 2, lh)
	local y = lh
	if self.ChildNodes and self.__expanded then
		for _, n in ipairs(self.__nodes) do if n.visible then n:SetPos(indent, 0); n:SetWide(w - indent); n:__layout() end end
		local cy = 0
		for _, n in ipairs(self.__nodes) do if n.visible then n:SetPos(indent, cy); cy = cy + n.h end end
		self.ChildNodes:SetPos(0, lh); self.ChildNodes:SetSize(w, cy)
		y = y + cy
	end
	if self.h ~= y then self:SetTall(y) end
end
function DTree_Node:Paint(w, h)
	local lh = self:GetLineHeight()
	if self.__selected then surface.SetDrawColor(SKIN.Colours.Tree.Selected); surface.DrawRect(0, 0, w, lh)
	elseif self.Label:IsHovered() then surface.SetDrawColor(Color(255, 255, 255, 60)); surface.DrawRect(0, 0, w, lh) end
end

local DTree = derma.DefineControl("DTree", "Tree", {}, "DScrollPanel")
function DTree:Init()
	DScrollPanel.Init(self)
	self.__indent, self.__line_h, self.__icons, self.__drag_click = 14, 17, true, false
	self.RootNode = vgui.Create("DTree_Node", self.pnlCanvas)
	self.RootNode:SetRoot(self.RootNode)
	self.RootNode.__tree = self
	self.RootNode:SetExpanded(true)
	self.RootNode.OnNodeSelected = function(_, n) self:OnNodeSelected(n) end
	self.RootNode.ShowIcons = function() return self.__icons end
	self.RootNode.GetLineHeight = function() return self.__line_h end
	self.RootNode.GetIndentSize = function() return self.__indent end
	self.RootNode.SetSelectedItem = function(_, n) self:SetSelectedItem(n) end
	self.RootNode:Dock(TOP)
	self.__paint_bg = true
	self.__bg_color = Color(255, 255, 255)
end
function DTree:Root() return self.RootNode end
function DTree:AddNode(name, icon) return self.RootNode:AddNode(name, icon) end
function DTree:ChildExpanded() end
function DTree:SetIndentSize(i) self.__indent = i end
function DTree:GetIndentSize() return self.__indent end
function DTree:SetLineHeight(h) self.__line_h = h end
function DTree:GetLineHeight() return self.__line_h end
function DTree:SetShowIcons(b) self.__icons = b ~= false end
function DTree:GetShowIcons() return self.__icons end
function DTree:ShowIcons() return self.__icons end
function DTree:SetClickOnDragHover(b) self.__drag_click = b == true end
function DTree:GetClickOnDragHover() return self.__drag_click end
function DTree:SetExpanded(b) self.RootNode:SetExpanded(b) end
function DTree:ExpandTo(b) end
function DTree:LayoutTree() self:InvalidateLayout(true) end
function DTree:MoveChildTo(n) self.RootNode:MoveChildTo(n) end
function DTree:GetSelectedItem() return self.__selected end
function DTree:SetSelectedItem(n)
	if self.__selected and self.__selected.__valid then self.__selected:SetSelected(false) end
	self.__selected = n
	if n then n:SetSelected(true); self:OnNodeSelected(n) end
end
function DTree:OnNodeSelected(n) end
function DTree:DoClick(n) return false end
function DTree:DoRightClick(n) return false end
function DTree:Paint(w, h) surface.SetDrawColor(self.__bg_color); surface.DrawRect(0, 0, w, h); surface.SetDrawColor(SKIN.colButtonBorder); surface.DrawOutlinedRect(0, 0, w, h) end

-- colour controls: DColorButton, DColorPalette, DAlphaBar, DRGBPicker, DColorCube, DColorMixer
local DColorButton = derma.DefineControl("DColorButton", "Colour swatch button", {}, "DLabel")
function DColorButton:Init() self.mouse_input = true; self.__color = Color(255, 255, 255); self.__border = true; self:SetText(""); self:SetSize(10, 10) end
function DColorButton:SetColor(c, no_tooltip) self.__color = c; if not no_tooltip then self:SetTooltip(string.format("%d %d %d", c.r, c.g, c.b)) end end
function DColorButton:GetColor() return self.__color end
function DColorButton:SetDrawBorder(b) self.__border = b ~= false end
function DColorButton:GetDrawBorder() return self.__border end
function DColorButton:SetID(i) self.__id = i end
function DColorButton:GetID() return self.__id end
function DColorButton:GetSelected() return self.__selected == true end
function DColorButton:Paint(w, h) surface.SetDrawColor(self.__color); surface.DrawRect(0, 0, w, h); if self.__border then surface.SetDrawColor(SKIN.colButtonBorder); surface.DrawOutlinedRect(0, 0, w, h) end end

local DColorPalette = derma.DefineControl("DColorPalette", "Colour palette", {}, "DIconLayout")
local DEFAULT_PALETTE = {}
do
	local hues = { Color(255, 0, 0), Color(255, 128, 0), Color(255, 255, 0), Color(128, 255, 0), Color(0, 255, 0), Color(0, 255, 128), Color(0, 255, 255), Color(0, 128, 255), Color(0, 0, 255), Color(128, 0, 255), Color(255, 0, 255), Color(255, 0, 128) }
	for row = 0, 7 do
		for _, c in ipairs(hues) do
			local f = 1 - row / 8
			DEFAULT_PALETTE[#DEFAULT_PALETTE + 1] = Color(math.floor(c.r * f + 255 * (1 - f) * (row < 4 and 0 or 1) * 0), math.floor(c.g * f), math.floor(c.b * f))
		end
	end
	for i = 0, 11 do local g = math.floor(255 * i / 11); DEFAULT_PALETTE[#DEFAULT_PALETTE + 1] = Color(g, g, g) end
end
function DColorPalette:Init()
	DIconLayout.Init(self)
	self.__button_size, self.__rows = 10, 8
	self.__space_x, self.__space_y = 1, 1
	self.Buttons = {}
	self:Reset()
end
function DColorPalette:Reset()
	for _, b in ipairs(self.Buttons) do b:Remove() end
	self.Buttons = {}
	for i, c in ipairs(DEFAULT_PALETTE) do
		local b = vgui.Create("DColorButton", self)
		b:SetSize(self.__button_size, self.__button_size)
		b:SetColor(c)
		b:SetID(i)
		b.DoClick = function() self:DoClick(c, b) end
		b.DoRightClick = function() self:OnRightClickButton(b) end
		self.Buttons[i] = b
	end
	self:InvalidateLayout()
end
function DColorPalette:ResetSavedColors() self:Reset() end
function DColorPalette:SetButtonSize(s) self.__button_size = s; for _, b in ipairs(self.Buttons) do b:SetSize(s, s) end self:InvalidateLayout() end
function DColorPalette:GetButtonSize() return self.__button_size end
function DColorPalette:SetNumRows(n) self.__rows = n end
function DColorPalette:GetNumRows() return self.__rows end
function DColorPalette:SetColorButtons(tbl) for i, c in pairs(tbl) do if self.Buttons[i] then self.Buttons[i]:SetColor(c) end end end
function DColorPalette:SetColor(c) self.__color = c end
function DColorPalette:SaveColor(btn, c) btn:SetColor(c) end
function DColorPalette:DoClick(c, btn) self:OnValueChanged(c); self:UpdateConVars(c) end
function DColorPalette:OnRightClickButton(btn) if self.__color then self:SaveColor(btn, self.__color) end end
function DColorPalette:OnValueChanged(c) end
function DColorPalette:NetworkColorChange() end
function DColorPalette:SetConVarR(c) self.__cv_r = c end
function DColorPalette:SetConVarG(c) self.__cv_g = c end
function DColorPalette:SetConVarB(c) self.__cv_b = c end
function DColorPalette:SetConVarA(c) self.__cv_a = c end
function DColorPalette:GetConVarR() return self.__cv_r end
function DColorPalette:GetConVarG() return self.__cv_g end
function DColorPalette:GetConVarB() return self.__cv_b end
function DColorPalette:GetConVarA() return self.__cv_a end
function DColorPalette:UpdateConVar(cv, part, c) if cv then RunConsoleCommand(cv, tostring(c[part])) end end
function DColorPalette:UpdateConVars(c) self:UpdateConVar(self.__cv_r, "r", c); self:UpdateConVar(self.__cv_g, "g", c); self:UpdateConVar(self.__cv_b, "b", c); self:UpdateConVar(self.__cv_a, "a", c) end

local DAlphaBar = derma.DefineControl("DAlphaBar", "Alpha bar", {}, "DPanel")
function DAlphaBar:Init() self.__value, self.__bar_color = 1, Color(255, 255, 255); self.mouse_input = true; self.__paint_bg = false end
function DAlphaBar:SetValue(v) self.__value = math.max(0, math.min(1, v or 0)) end
function DAlphaBar:GetValue() return self.__value end
function DAlphaBar:SetBarColor(c) self.__bar_color = c end
function DAlphaBar:GetBarColor() return self.__bar_color end
function DAlphaBar:OnChange(v) end
function DAlphaBar:OnMousePressed() self.__drag = true; self:MouseCapture(true); self:Think() end
function DAlphaBar:OnMouseReleased() self.__drag = false; self:MouseCapture(false) end
function DAlphaBar:Think() if not self.__drag then return end local _, ly = self:CursorPos(); local v = 1 - math.max(0, math.min(1, ly / math.max(1, self.h))); if v ~= self.__value then self.__value = v; self:OnChange(v) end end
function DAlphaBar:Paint(w, h)
	for i = 0, h - 1 do local a = 1 - i / h; surface.SetDrawColor(Color(self.__bar_color.r, self.__bar_color.g, self.__bar_color.b, a * 255)); surface.DrawRect(0, i, w, 1) end
	surface.SetDrawColor(Color(0, 0, 0)); surface.DrawRect(0, (1 - self.__value) * (h - 1), w, 1)
end

local function hsv(h, s, v)
	h = (h % 360) / 60
	local c = v * s
	local x = c * (1 - math.abs(h % 2 - 1))
	local r, g, b
	if h < 1 then r, g, b = c, x, 0 elseif h < 2 then r, g, b = x, c, 0 elseif h < 3 then r, g, b = 0, c, x elseif h < 4 then r, g, b = 0, x, c elseif h < 5 then r, g, b = x, 0, c else r, g, b = c, 0, x end
	local m = v - c
	return Color(math.floor((r + m) * 255 + 0.5), math.floor((g + m) * 255 + 0.5), math.floor((b + m) * 255 + 0.5))
end
local function rgb_to_hsv(c)
	local r, g, b = c.r / 255, c.g / 255, c.b / 255
	local mx, mn = math.max(r, g, b), math.min(r, g, b)
	local d = mx - mn
	local h = 0
	if d > 0 then
		if mx == r then h = 60 * (((g - b) / d) % 6) elseif mx == g then h = 60 * ((b - r) / d + 2) else h = 60 * ((r - g) / d + 4) end
	end
	return h, mx > 0 and d / mx or 0, mx
end
HSVToColor = HSVToColor or hsv
ColorToHSV = ColorToHSV or rgb_to_hsv

local DRGBPicker = derma.DefineControl("DRGBPicker", "Hue bar", {}, "DPanel")
function DRGBPicker:Init() self.__hue = 0; self.mouse_input = true; self.__paint_bg = false end
function DRGBPicker:GetPosColor(x, y) local h = math.max(0, math.min(1, y / math.max(1, self.h))) * 360; return hsv(h, 1, 1), x, y end
function DRGBPicker:GetRGB() return hsv(self.__hue, 1, 1) end
function DRGBPicker:SetRGB(c) self.__hue = (rgb_to_hsv(c)) end
function DRGBPicker:OnChange(c) end
function DRGBPicker:OnMousePressed() self.__drag = true; self:MouseCapture(true); self:Think() end
function DRGBPicker:OnMouseReleased() self.__drag = false; self:MouseCapture(false) end
function DRGBPicker:Think() if not self.__drag then return end local _, ly = self:CursorPos(); local h = math.max(0, math.min(1, ly / math.max(1, self.h))) * 360; if h ~= self.__hue then self.__hue = h; self:OnChange(self:GetRGB()) end end
function DRGBPicker:Paint(w, h)
	for i = 0, h - 1 do surface.SetDrawColor(hsv(i / h * 360, 1, 1)); surface.DrawRect(0, i, w, 1) end
	surface.SetDrawColor(Color(0, 0, 0)); surface.DrawRect(0, self.__hue / 360 * (h - 1), w, 1)
end

local DColorCube = derma.DefineControl("DColorCube", "Saturation / value square", {}, "DSlider")
function DColorCube:Init() DSlider.Init(self); self.__hue, self.__base = 0, Color(255, 0, 0); self.__x, self.__y = 1, 0; self.__default = Color(255, 255, 255); self.Knob:SetSize(9, 9) end
function DColorCube:SetHue(h) self.__hue = h; self.__base = hsv(h, 1, 1) end
function DColorCube:GetHue() return self.__hue end
function DColorCube:SetBaseRGB(c) self.__base = c; self.__hue = (rgb_to_hsv(c)); self:UpdateColor() end
function DColorCube:GetBaseRGB() return self.__base end
function DColorCube:SetRGB(c) local h, s, v = rgb_to_hsv(c); self:SetHue(h); self.__x, self.__y = s, 1 - v; self:UpdateColor() end
function DColorCube:SetColor(c) self:SetRGB(c) end
function DColorCube:GetRGB() return self.__rgb or hsv(self.__hue, self.__x, 1 - self.__y) end
function DColorCube:SetDefaultColor(c) self.__default = c end
function DColorCube:GetDefaultColor() return self.__default end
function DColorCube:ResetToDefaultValue() self:SetRGB(self.__default); self:OnUserChanged(self:GetRGB()) end
function DColorCube:UpdateColor() self.__rgb = hsv(self.__hue, self.__x, 1 - self.__y); self:InvalidateLayout() end
function DColorCube:TranslateValues(x, y) return x, y end
function DColorCube:OnUserChanged(c) end
function DColorCube:Think()
	if not self.__dragging then return end
	local lx, ly = self:CursorPos()
	local x, y = math.max(0, math.min(1, lx / math.max(1, self.w))), math.max(0, math.min(1, ly / math.max(1, self.h)))
	if x ~= self.__x or y ~= self.__y then self.__x, self.__y = x, y; self:UpdateColor(); self:OnUserChanged(self:GetRGB()) end
end
function DColorCube:PerformLayout(w, h) self.Knob:SetPos(self.__x * w - 4, self.__y * h - 4) end
function DColorCube:Paint(w, h)
	-- the square: hue at full saturation across x, value down y, in 8 px cells
	local step = 8
	for y = 0, h - 1, step do
		for x = 0, w - 1, step do
			surface.SetDrawColor(hsv(self.__hue, (x + step / 2) / w, 1 - (y + step / 2) / h))
			surface.DrawRect(x, y, math.min(step, w - x), math.min(step, h - y))
		end
	end
end

local DColorMixer = derma.DefineControl("DColorMixer", "Colour mixer", {}, "DPanel")
function DColorMixer:Init()
	self.__paint_bg = false
	self.__color = Color(255, 255, 255, 255)
	self.__alpha_bar, self.__palette_on, self.__wangs_on = true, true, true
	self.Palette = vgui.Create("DColorPalette", self)
	self.Palette.OnValueChanged = function(_, c) self:SetColor(Color(c.r, c.g, c.b, self.__color.a)); self:ValueChanged(self.__color) end
	self.Cube = vgui.Create("DColorCube", self)
	self.Cube.OnUserChanged = function(_, c) self.__color = Color(c.r, c.g, c.b, self.__color.a); self:UpdateWangs(); self:ValueChanged(self.__color) end
	self.RGB = vgui.Create("DRGBPicker", self)
	self.RGB.OnChange = function(_, c) self.Cube:SetBaseRGB(c); local cc = self.Cube:GetRGB(); self.__color = Color(cc.r, cc.g, cc.b, self.__color.a); self:UpdateWangs(); self:ValueChanged(self.__color) end
	self.Alpha = vgui.Create("DAlphaBar", self)
	self.Alpha.OnChange = function(_, v) self.__color.a = math.floor(v * 255 + 0.5); self:UpdateWangs(); self:ValueChanged(self.__color) end
	self.WangsPanel = vgui.Create("DPanel", self)
	self.WangsPanel.__paint_bg = false
	self.Wangs = {}
	for i, k in ipairs({ "r", "g", "b", "a" }) do
		local w = vgui.Create("DNumberWang", self.WangsPanel)
		w:SetMinMax(0, 255)
		w:SetTall(20)
		w.OnValueChanged = function(_, v) self.__color[k] = v; self:UpdateColor(self.__color); self:ValueChanged(self.__color) end
		self.Wangs[k] = w
	end
	self:SetSize(256, 186)
	self:UpdateColor(self.__color)
end
function DColorMixer:SetAlphaBar(b) self.__alpha_bar = b ~= false; self:InvalidateLayout() end
function DColorMixer:GetAlphaBar() return self.__alpha_bar end
function DColorMixer:SetPalette(b) self.__palette_on = b ~= false; self:InvalidateLayout() end
function DColorMixer:GetPalette() return self.__palette_on end
function DColorMixer:SetWangs(b) self.__wangs_on = b ~= false; self:InvalidateLayout() end
function DColorMixer:GetWangs() return self.__wangs_on end
function DColorMixer:SetLabel(t) self.__label = t end
function DColorMixer:SetPaletteName(n) end
function DColorMixer:SetColor(c) self.__color = Color(c.r, c.g, c.b, c.a or 255); self:UpdateColor(self.__color) end
function DColorMixer:GetColor() return self.__color end
function DColorMixer:SetBaseColor(c) self.Cube:SetBaseRGB(c) end
function DColorMixer:SetVector(v) self:SetColor(Color(v.x * 255, v.y * 255, v.z * 255, 255)) end
function DColorMixer:GetVector() return Vector(self.__color.r / 255, self.__color.g / 255, self.__color.b / 255) end
function DColorMixer:UpdateColor(c)
	self.Cube:SetRGB(c)
	self.RGB:SetRGB(c)
	self.Alpha:SetValue((c.a or 255) / 255)
	self.Alpha:SetBarColor(Color(c.r, c.g, c.b))
	self:UpdateWangs()
end
function DColorMixer:UpdateWangs() for k, w in pairs(self.Wangs) do if not w:IsEditing() then w:SetText(tostring(math.floor(self.__color[k] or 255))) end end end
function DColorMixer:UpdateDefaultColor() end
function DColorMixer:ValueChanged(c) self:UpdateConVars(c) end
function DColorMixer:SetConVarR(c) self.__cv_r = c end
function DColorMixer:SetConVarG(c) self.__cv_g = c end
function DColorMixer:SetConVarB(c) self.__cv_b = c end
function DColorMixer:SetConVarA(c) self.__cv_a = c end
function DColorMixer:GetConVarR() return self.__cv_r end
function DColorMixer:GetConVarG() return self.__cv_g end
function DColorMixer:GetConVarB() return self.__cv_b end
function DColorMixer:GetConVarA() return self.__cv_a end
function DColorMixer:UpdateConVar(cv, part, c) if cv then RunConsoleCommand(cv, tostring(math.floor(c[part] or 255))) end end
function DColorMixer:UpdateConVars(c) self:UpdateConVar(self.__cv_r, "r", c); self:UpdateConVar(self.__cv_g, "g", c); self:UpdateConVar(self.__cv_b, "b", c); self:UpdateConVar(self.__cv_a, "a", c) end
function DColorMixer:ConVarThink(cv, part)
	if not cv then return end
	local v = GetConVar(cv)
	if v and v:GetInt() ~= math.floor(self.__color[part] or 255) and not self.__dirty_from_cv then self.__color[part] = v:GetInt(); self:UpdateColor(self.__color) end
end
function DColorMixer:DoConVarThink(cv, part) self:ConVarThink(cv, part) end
function DColorMixer:Think() self:ConVarThink(self.__cv_r, "r"); self:ConVarThink(self.__cv_g, "g"); self:ConVarThink(self.__cv_b, "b"); self:ConVarThink(self.__cv_a, "a") end
function DColorMixer:PerformLayout(w, h)
	local y = 0
	if self.__wangs_on then
		self.WangsPanel:SetVisible(true); self.WangsPanel:SetPos(0, 0); self.WangsPanel:SetSize(w, 20)
		local ww = math.floor(w / 4)
		for i, k in ipairs({ "r", "g", "b", "a" }) do self.Wangs[k]:SetPos((i - 1) * ww, 0); self.Wangs[k]:SetWide(ww - 2); self.Wangs[k]:SetVisible(k ~= "a" or self.__alpha_bar) end
		y = 24
	else self.WangsPanel:SetVisible(false) end
	local ph = self.__palette_on and 32 or 0
	local cube_h = h - y - ph
	local right = (self.__alpha_bar and 16 or 0) + 16
	self.Cube:SetPos(0, y); self.Cube:SetSize(w - right - 4, cube_h)
	self.RGB:SetPos(w - right, y); self.RGB:SetSize(14, cube_h)
	self.Alpha:SetVisible(self.__alpha_bar)
	self.Alpha:SetPos(w - 14, y); self.Alpha:SetSize(14, cube_h)
	self.Palette:SetVisible(self.__palette_on)
	if self.__palette_on then self.Palette:SetButtonSize(math.floor((w - 11) / 12)); self.Palette:SetPos(0, h - ph + 2); self.Palette:SetSize(w, ph - 2) end
end

-- the small remaining ones
local DBinder = derma.DefineControl("DBinder", "Key binder", {}, "DButton")
function DBinder:Init() self.__key = 0; self:SetText("NONE"); self.keyboard_input = true end
function DBinder:SetSelectedNumber(k) self.__key = k or 0; self:UpdateText() end
function DBinder:GetSelectedNumber() return self.__key end
function DBinder:SetValue(k) self:SetSelectedNumber(k); self:OnChange(k) end
function DBinder:GetValue() return self.__key end
function DBinder:UpdateText() local name = input and input.GetKeyName and input.GetKeyName(self.__key) or tostring(self.__key); self:SetText(self.__key == 0 and "NONE" or (name and name:upper() or tostring(self.__key))) end
function DBinder:OnChange(k) end
function DBinder:DoClick() self.__listening = true; self:SetText("PRESS A KEY"); self:RequestFocus() end
function DBinder:OnKeyCodePressed(code) if self.__listening then self.__listening = false; self:SetValue(code == KEY_ESCAPE and 0 or code); self:ConVarChanged(self.__key) return true end end
function DBinder:OnMousePressed(code) if self.__listening and code ~= MOUSE_LEFT then self.__listening = false; self:SetValue(code); return end DButton.OnMousePressed(self, code) end

local DLabelURL = derma.DefineControl("DLabelURL", "URL label", {}, "DLabel")
function DLabelURL:Init() self.mouse_input = true; self.__text_color = Color(0, 0, 255); self.__cursor = "hand"; self.__auto_v = false end
function DLabelURL:SetURL(u) self.__url = u end
function DLabelURL:GetURL() return self.__url end
function DLabelURL:DoClick() if self.__url and gui and gui.OpenURL then gui.OpenURL(self.__url) end end
function DLabelURL:SetTextStyleColor(c) self.__text_color = c end
function DLabelURL:GetTextStyleColor() return self.__text_color end
function DLabelURL:SetAutoStretchVertical(b) self.__auto_v = b == true end
function DLabelURL:GetAutoStretchVertical() return self.__auto_v end
function DLabelURL:UpdateFGColor() end
function DLabelURL:SetColor(c) self.__text_color = c end
function DLabelURL:GetColor() return self.__text_color end
function DLabelURL:SetTextColor(c) self.__text_color = c end
function DLabelURL:GetTextColor() return self.__text_color end

local DMenuOptionCVar = derma.DefineControl("DMenuOptionCVar", "Menu option bound to a convar", {}, "DMenuOption")
function DMenuOptionCVar:Init() DMenuOption.Init(self); self:SetIsCheckable(true); self.__on, self.__off = "1", "0" end
function DMenuOptionCVar:SetConVar(c) self.__convar = c end
function DMenuOptionCVar:GetConVar() return self.__convar end
function DMenuOptionCVar:SetValueOn(v) self.__on = tostring(v) end
function DMenuOptionCVar:GetValueOn() return self.__on end
function DMenuOptionCVar:SetValueOff(v) self.__off = tostring(v) end
function DMenuOptionCVar:GetValueOff() return self.__off end
function DMenuOptionCVar:Think() if self.__convar then local cv = GetConVar(self.__convar); if cv then self:SetChecked(cv:GetString() == self.__on) end end end
function DMenuOptionCVar:OnChecked(b) if self.__convar then RunConsoleCommand(self.__convar, b and self.__on or self.__off) end end

local DSprite = derma.DefineControl("DSprite", "Rotating sprite", {}, "DPanel")
function DSprite:Init() self.__paint_bg = false; self.__color, self.__rot = Color(255, 255, 255), 0; self.mouse_input = false end
function DSprite:SetMaterial(m) self.__mat = m end
function DSprite:GetMaterial() return self.__mat end
function DSprite:SetColor(c) self.__color = c end
function DSprite:GetColor() return self.__color end
function DSprite:SetRotation(r) self.__rot = r end
function DSprite:GetRotation() return self.__rot end
function DSprite:SetHandle(v) self.__handle = v end
function DSprite:GetHandle() return self.__handle end
function DSprite:Paint(w, h) if not self.__mat then return end surface.SetMaterial(self.__mat); surface.SetDrawColor(self.__color); surface.DrawTexturedRectRotated(w / 2, h / 2, w, h, self.__rot) end

local DShape = derma.DefineControl("DShape", "Shape", {}, "DPanel")
function DShape:Init() self.__paint_bg = false; self.__type, self.__color, self.__border_color = "Rect", Color(255, 255, 255), Color(0, 0, 0, 0); self.mouse_input = false end
function DShape:SetType(t) self.__type = t end
function DShape:GetType() return self.__type end
function DShape:SetColor(c) self.__color = c end
function DShape:GetColor() return self.__color end
function DShape:SetBorderColor(c) self.__border_color = c end
function DShape:GetBorderColor() return self.__border_color end
function DShape:Paint(w, h) surface.SetDrawColor(self.__color); surface.DrawRect(0, 0, w, h); if (self.__border_color.a or 0) > 0 then surface.SetDrawColor(self.__border_color); surface.DrawOutlinedRect(0, 0, w, h) end end

local DDrawer = derma.DefineControl("DDrawer", "Sliding drawer", {}, "DPanel")
function DDrawer:Init() self.__open_size, self.__open_time, self.__open = 100, 0.3, false; self:SetTall(0) end
function DDrawer:SetOpenSize(s) self.__open_size = s end
function DDrawer:GetOpenSize() return self.__open_size end
function DDrawer:SetOpenTime(t) self.__open_time = t end
function DDrawer:GetOpenTime() return self.__open_time end
function DDrawer:Open() self.__open = true; self:SizeTo(-1, self.__open_size, self.__open_time) end
function DDrawer:Close() self.__open = false; self:SizeTo(-1, 0, self.__open_time) end
function DDrawer:Toggle() if self.__open then self:Close() else self:Open() end end

local DExpandButton = derma.DefineControl("DExpandButton", "Expand toggle", {}, "DButton")
function DExpandButton:Init() self.__expanded = false; self:SetText(""); self:SetSize(15, 15) end
function DExpandButton:SetExpanded(b) self.__expanded = b == true end
function DExpandButton:GetExpanded() return self.__expanded end
function DExpandButton:Paint(w, h) surface.SetDrawColor(Color(0, 0, 0)); surface.DrawOutlinedRect(2, 2, w - 4, h - 4); surface.DrawRect(4, h / 2, w - 8, 1); if not self.__expanded then surface.DrawRect(w / 2, 4, 1, h - 8) end end

local DKillIcon = derma.DefineControl("DKillIcon", "Kill icon", {}, "DPanel")
function DKillIcon:Init() self.__paint_bg = false; self.__name = "default" end
function DKillIcon:SetName(n) self.__name = n end
function DKillIcon:GetName() return self.__name end
function DKillIcon:Paint(w, h) if killicon and killicon.Draw then killicon.Draw(w / 2, h / 2, self.__name, 255) end end

local DSizeToContents = derma.DefineControl("DSizeToContents", "Sizes to its children", {}, "Panel")
function DSizeToContents:Init() self.__sx, self.__sy = true, true end
function DSizeToContents:SetSizeX(b) self.__sx = b == true end
function DSizeToContents:GetSizeX() return self.__sx end
function DSizeToContents:SetSizeY(b) self.__sy = b == true end
function DSizeToContents:GetSizeY() return self.__sy end
function DSizeToContents:PerformLayout(w, h) self:SizeToChildren(self.__sx, self.__sy) end

local DDragBase = derma.DefineControl("DDragBase", "Drag and drop base", {}, "DPanel")
function DDragBase:Init() self.__paint_bg = false; self.__dnd, self.__readonly, self.__live = nil, false, false end
function DDragBase:MakeDroppable(name, allow_copy) self.__dnd = name end
function DDragBase:SetDnD(n) self.__dnd = n end
function DDragBase:GetDnD() return self.__dnd end
function DDragBase:SetReadOnly(b) self.__readonly = b == true end
function DDragBase:GetReadOnly() return self.__readonly end
function DDragBase:SetUseLiveDrag(b) self.__live = b == true end
function DDragBase:GetUseLiveDrag() return self.__live end
function DDragBase:SetDropPos(p) self.__drop_pos = p end
function DDragBase:UpdateDropTarget() end
function DDragBase:OnModified() end
function DDragBase:DropAction_Normal() end
function DDragBase:DropAction_Copy() end
function DDragBase:DropAction_Simple() end

local DNotify = derma.DefineControl("DNotify", "Notification stack", {}, "Panel")
function DNotify:Init() self.Items = {}; self.__life, self.__spacing, self.__align = 5, 2, 7; self.mouse_input = false end
function DNotify:SetLife(l) self.__life = l end
function DNotify:GetLife() return self.__life end
function DNotify:SetSpacing(s) self.__spacing = s end
function DNotify:GetSpacing() return self.__spacing end
function DNotify:SetAlignment(a) self.__align = a end
function DNotify:GetAlignment() return self.__align end
function DNotify:GetItems() return self.Items end
function DNotify:AddItem(p, life) p:SetParent(self); self.Items[#self.Items + 1] = { panel = p, dies = SysTime() + (life or self.__life) }; self:InvalidateLayout() end
function DNotify:Shuffle()
	local y = 0
	for i = #self.Items, 1, -1 do
		local it = self.Items[i]
		if not it.panel.__valid or SysTime() > it.dies then if it.panel.__valid then it.panel:Remove() end table.remove(self.Items, i)
		else it.panel:SetPos(self.__align % 3 == 0 and self.w - it.panel.w or 0, y); y = y + it.panel.h + self.__spacing end
	end
end
function DNotify:Think() self:Shuffle() end

local DNumberScratch = derma.DefineControl("DNumberScratch", "Scratch number", {}, "DImageButton")
function DNumberScratch:Init() self.__min, self.__max, self.__decimals, self.__value, self.__zoom = 0, 1, 0, 0, 1; self.__active = false; self:SetSize(16, 16) end
function DNumberScratch:SetMin(m) self.__min = m end
function DNumberScratch:SetMax(m) self.__max = m end
function DNumberScratch:GetMin() return self.__min end
function DNumberScratch:GetMax() return self.__max end
function DNumberScratch:GetRange() return self.__max - self.__min end
function DNumberScratch:SetDecimals(d) self.__decimals = d end
function DNumberScratch:GetDecimals() return self.__decimals end
function DNumberScratch:SetValue(v) v = math.max(self.__min, math.min(self.__max, tonumber(v) or 0)); if v ~= self.__value then self.__value = v; self:OnValueChanged(v); self:ConVarChanged(v) end end
function DNumberScratch:GetFloatValue() return self.__value end
function DNumberScratch:SetFloatValue(v) self:SetValue(v) end
function DNumberScratch:GetTextValue() return self.__decimals > 0 and string.format("%." .. self.__decimals .. "f", self.__value) or tostring(math.floor(self.__value)) end
function DNumberScratch:GetFraction(v) return ((v or self.__value) - self.__min) / math.max(1e-9, self:GetRange()) end
function DNumberScratch:SetFraction(f) self:SetValue(self.__min + self:GetRange() * f) end
function DNumberScratch:SetZoom(z) self.__zoom = z end
function DNumberScratch:GetZoom() return self.__zoom end
function DNumberScratch:IdealZoom() return 1 end
function DNumberScratch:SetActive(b) self.__active = b == true end
function DNumberScratch:GetActive() return self.__active end
function DNumberScratch:IsEditing() return self.__active end
function DNumberScratch:SetShouldDrawScreen(b) self.__screen = b == true end
function DNumberScratch:GetShouldDrawScreen() return self.__screen == true end
function DNumberScratch:LockCursor() end
function DNumberScratch:DrawNotches() end
function DNumberScratch:DrawScreen() end
function DNumberScratch:PaintScratchWindow() end
function DNumberScratch:UpdateConVar() self:ConVarChanged(self.__value) end
function DNumberScratch:OnValueChanged(v) end
function DNumberScratch:OnMousePressed() self.__active = true; self.__start_x, self.__start_v = cursor_x, self.__value; self:MouseCapture(true) end
function DNumberScratch:OnMouseReleased() self.__active = false; self:MouseCapture(false) end
function DNumberScratch:Think() if not self.__active then return end self:SetValue(self.__start_v + (cursor_x - self.__start_x) / 100 * self:GetRange() * self.__zoom) end
function DNumberScratch:Paint(w, h) surface.SetDrawColor(SKIN.control_color); surface.DrawRect(0, 0, w, h); surface.SetDrawColor(SKIN.colButtonBorder); surface.DrawOutlinedRect(0, 0, w, h) end

local DHTML = derma.DefineControl("DHTML", "HTML view (no browser yet)", {}, "DPanel")
function DHTML:Init() self.__scrollbars, self.__allow_lua, self.__functions = true, false, {} end
function DHTML:SetScrollbars(b) self.__scrollbars = b ~= false end
function DHTML:GetScrollbars() return self.__scrollbars end
function DHTML:SetAllowLua(b) self.__allow_lua = b == true end
function DHTML:GetAllowLua() return self.__allow_lua end
function DHTML:AddFunction(lib, name, fn) self.__functions[lib .. "." .. name] = fn end
function DHTML:Call(js) end
function DHTML:QueueJavascript(js) end
function DHTML:RunJavascript(js) end
function DHTML:OpenURL(u) self.__url = u end
function DHTML:SetHTML(h) self.__html = h end
function DHTML:Paint(w, h) surface.SetDrawColor(Color(255, 255, 255)); surface.DrawRect(0, 0, w, h) end

local DModelPanel = derma.DefineControl("DModelPanel", "Model view", {}, "DPanel")
function DModelPanel:Init() self.__paint_bg = false; self.__fov, self.__cam, self.__look_ang, self.__color, self.__ambient = 70, Vector(50, 50, 50), nil, Color(255, 255, 255), Color(50, 50, 50); self.__anim_speed, self.__animated = 1, false end
function DModelPanel:SetModel(m) self.__model = m; self.Entity = { GetModel = function() return m end, IsValid = function() return true end, SetAngles = function(e, a) self.__ent_ang = a end, GetAngles = function() return self.__ent_ang or Angle() end, SetPos = function() end, GetPos = function() return Vector() end, SetSequence = function() end, SetSkin = function() end, SetBodygroup = function() end, SetEyeTarget = function() end, SetAnimation = function() end, SetModel = function(e, mm) self.__model = mm end, Remove = function() end, LookupSequence = function() return -1 end, LookupBone = function() return nil end, GetBonePosition = function() return Vector(), Angle() end, SetMaterial = function() end, SetColor = function() end, FrameAdvance = function() end, SetCycle = function() end, GetCycle = function() return 0 end, SetPlaybackRate = function() end, SetIK = function() end, SetSubMaterial = function() end, SetPoseParameter = function() end, SetFlexWeight = function() end, GetFlexNum = function() return 0 end, OBBMins = function() return Vector(-16, -16, 0) end, OBBMaxs = function() return Vector(16, 16, 72) end, GetBoneCount = function() return 0 end } end
function DModelPanel:GetModel() return self.__model end
function DModelPanel:GetEntity() return self.Entity end
function DModelPanel:SetEntity(e) self.Entity = e end
function DModelPanel:SetFOV(f) self.__fov = f end
function DModelPanel:GetFOV() return self.__fov end
function DModelPanel:SetCamPos(p) self.__cam = p end
function DModelPanel:GetCamPos() return self.__cam end
function DModelPanel:SetLookAt(p) self.__look_at = p end
function DModelPanel:GetLookAt() return self.__look_at end
function DModelPanel:SetLookAng(a) self.__look_ang = a end
function DModelPanel:GetLookAng() return self.__look_ang end
function DModelPanel:SetColor(c) self.__color = c end
function DModelPanel:GetColor() return self.__color end
function DModelPanel:SetAmbientLight(c) self.__ambient = c end
function DModelPanel:GetAmbientLight() return self.__ambient end
function DModelPanel:SetDirectionalLight(dir, c) self.__lights = self.__lights or {}; self.__lights[dir] = c end
function DModelPanel:SetAnimSpeed(s) self.__anim_speed = s end
function DModelPanel:GetAnimSpeed() return self.__anim_speed end
function DModelPanel:SetAnimated(b) self.__animated = b == true end
function DModelPanel:GetAnimated() return self.__animated end
function DModelPanel:RunAnimation() end
function DModelPanel:StartScene(name) end
function DModelPanel:LayoutEntity(e) if self.__animated then self:RunAnimation() end end
function DModelPanel:PreDrawModel(e) return true end
function DModelPanel:PostDrawModel(e) end
function DModelPanel:DrawModel()
	-- the model is drawn by the client through a 3D2D plane in this panel's rectangle
	if not self.__model then return end
	local sx, sy = surface.__get_origin()
	hostdraw_model_in_panel(self.__model, sx, sy, self.w, self.h, self.__ent_ang)
end
function DModelPanel:Paint(w, h)
	if not self.Entity then return end
	self:LayoutEntity(self.Entity)
	if self:PreDrawModel(self.Entity) ~= false then self:DrawModel() end
	self:PostDrawModel(self.Entity)
end
function hostdraw_model_in_panel(model, x, y, w, h, ang)
	-- not drawn in the 2D layer yet: the panel shows the model name (open)
	surface.SetFont("DermaDefault")
	surface.SetTextColor(Color(255, 255, 255, 120))
	surface.SetTextPos(4, 4)
	surface.DrawText(model)
end

local DButtonSelect = derma.DefineControl("DButtonSelect", "Toggle button", {}, "DButton")
function DButtonSelect:Init() self.__toggle = true end
local DNumPad = derma.DefineControl("DNumPad", "Numpad key binder", {}, "DBinder")
local DPanelOverlay = derma.DefineControl("DPanelOverlay", "Panel overlay", {}, "DPanel")
function DPanelOverlay:Init() self.__paint_bg = false; self.mouse_input = false; self.__type, self.__color = 1, Color(255, 255, 255) end
function DPanelOverlay:SetType(t) self.__type = t end
function DPanelOverlay:GetType() return self.__type end
function DPanelOverlay:SetColor(c) self.__color = c end
function DPanelOverlay:GetColor() return self.__color end
function DPanelOverlay:Paint(w, h) surface.SetDrawColor(self.__color); surface.DrawOutlinedRect(0, 0, w, h, 2) end
local DPanelSelect = derma.DefineControl("DPanelSelect", "Selectable panels", {}, "DPanelList")
function DPanelSelect:Init() DPanelList.Init(self); self.__selected = nil; self.__convars = {} end
function DPanelSelect:AddPanel(p, convars) self:AddItem(p); p.__select_convars = convars; p.OnMousePressed = function() self:SelectPanel(p) end end
function DPanelSelect:SelectPanel(p) self.__selected = p; if p.__select_convars then for k, v in pairs(p.__select_convars) do RunConsoleCommand(k, tostring(v)) end end self:OnActivePanelChanged(p) end
function DPanelSelect:OnActivePanelChanged(p) end
function DPanelSelect:PerformLayout(w, h)
	local x, y, row_h = 0, 0, 0
	for _, p in ipairs(self.Items) do
		if x + p.w > self.pnlCanvas.w and x > 0 then x = 0; y = y + row_h; row_h = 0 end
		p:SetPos(x, y); x = x + p.w; row_h = math.max(row_h, p.h)
	end
	if self.__autosize then self:SetTall(y + row_h) end
	DScrollPanel.PerformLayout(self, w, h)
end
local DProgressBar = derma.DefineControl("DProgressBar", "Progress bar", {}, "DProgress")
local DTreeScroller = derma.DefineControl("DTreeScroller", "Tree scroller", {}, "DScrollPanel")
local DHorizontalScroller = derma.DefineControl("DHorizontalScroller", "Horizontal scroller", {}, "DPanel")
function DHorizontalScroller:Init() self.__paint_bg = false; self.Panels = {}; self.__offset, self.__overlap = 0, 0; self.pnlCanvas = vgui.Create("Panel", self) end
function DHorizontalScroller:AddPanel(p) p:SetParent(self.pnlCanvas); self.Panels[#self.Panels + 1] = p; self:InvalidateLayout() end
function DHorizontalScroller:GetCanvas() return self.pnlCanvas end
function DHorizontalScroller:SetOverlap(o) self.__overlap = o end
function DHorizontalScroller:GetOverlap() return self.__overlap end
function DHorizontalScroller:SetScroll(s) self.__offset = s; self:InvalidateLayout() end
function DHorizontalScroller:GetScroll() return self.__offset end
function DHorizontalScroller:ScrollToChild(p) self.__offset = p.x; self:InvalidateLayout() end
function DHorizontalScroller:SetUseLiveDrag() end
function DHorizontalScroller:MakeDroppable() end
function DHorizontalScroller:OnDragModified() end
function DHorizontalScroller:OnMouseWheeled(d) self.__offset = math.max(0, self.__offset - d * 30); self:InvalidateLayout(); return true end
function DHorizontalScroller:PerformLayout(w, h)
	local x = 0
	for _, p in ipairs(self.Panels) do p:SetPos(x, 0); p:SetTall(h); x = x + p.w - self.__overlap end
	self.__offset = math.max(0, math.min(self.__offset, math.max(0, x - w)))
	self.pnlCanvas:SetPos(-self.__offset, 0); self.pnlCanvas:SetSize(math.max(x, w), h)
end
local DTree_Node_Button = derma.DefineControl("DTree_Node_Button", "Tree node button", {}, "DButton")
local DScrollBarGrip = derma.DefineControl("DScrollBarGrip", "Scroll bar grip", {}, "DPanel")
local DTextEditor = derma.DefineControl("DTextEditor", "Multiline text", {}, "DTextEntry")
function DTextEditor:Init() DTextEntry.Init(self); self:SetMultiline(true) end
local DColorCombo = derma.DefineControl("DColorCombo", "Colour mixer in a property sheet", {}, "DPropertySheet")
function DColorCombo:Init() DPropertySheet.Init(self); self.Mixer = vgui.Create("DColorMixer"); self.Mixer.ValueChanged = function(_, c) self:OnValueChanged(c) end; self:AddSheet("Color", self.Mixer); self.Palette = vgui.Create("DColorPalette"); self.Palette.OnValueChanged = function(_, c) self.Mixer:SetColor(c); self:OnValueChanged(c) end; self:AddSheet("Palette", self.Palette) end
function DColorCombo:SetColor(c) self.Mixer:SetColor(c) end
function DColorCombo:GetColor() return self.Mixer:GetColor() end
function DColorCombo:SetConVarR(c) self.Mixer:SetConVarR(c) end
function DColorCombo:SetConVarG(c) self.Mixer:SetConVarG(c) end
function DColorCombo:SetConVarB(c) self.Mixer:SetConVarB(c) end
function DColorCombo:SetConVarA(c) self.Mixer:SetConVarA(c) end
function DColorCombo:OnValueChanged(c) end
function DColorCombo:IsEditing() return false end
function DColorCombo:BuildControls() end

-- documented panels not built yet: creatable as a plain DPanel so addons keep running
for _, n in ipairs({ "DColorMixer", "DColorPalette", "DColorButton", "DColorCube", "DAlphaBar", "DRGBPicker", "DColorCombo", "DListView", "DListView_Line", "DListView_Column", "DListBox", "DListBoxItem", "DTree", "DTree_Node", "DPropertySheet", "DTab", "DCollapsibleCategory", "DCategoryList", "DForm", "DIconLayout", "DGrid", "DTooltip", "DHTML", "DHTMLControls", "DModelPanel", "DModelSelect", "DAdjustableModelPanel", "DImageButton", "DSpawnIcon", "DIconBrowser", "DFileBrowser", "DEntityProperties", "DProperties", "DProperty_Generic", "DProperty_Boolean", "DProperty_Combo", "DProperty_Float", "DProperty_Int", "DProperty_VectorColor", "DNumberWang", "DNumberScratch", "DNumPad", "DBinder", "DButtonSelect", "DHorizontalDivider", "DHorizontalScroller", "DVerticalDivider", "DDragBase", "DDrawer", "DExpandButton", "DKillIcon", "DLabelEditable", "DLabelURL", "DMenuBar", "DMenuOptionCVar", "DNotify", "DPanelList", "DPanelOverlay", "DPanelSelect", "DProgressBar", "DScrollBarGrip", "DShape", "DSizeToContents", "DSprite", "DTextEditor", "DTileLayout", "DTreeScroller", "DTree_Node_Button", "DVScrollBar", "ContentIcon", "ContentHeader", "ContentContainer", "ContentSidebar", "ContentSidebarToolbox", "SpawnIcon", "SpawnmenuContentPanel", "ControlPanel", "ControlPresets", "PresetEditor", "PropSelect", "MatSelect", "ImageCheckBox", "Material", "AvatarImage", "RichText", "RadioButton", "CheckButton", "Frame", "Label", "Button", "TextEntry", "HTML", "Slider", "ImagePanel", "ScrollBar", "ScrollBarGrip", "Divider", "GModNotify", "ToolMenu", "ToolPanel", "DAlphaBar", "SlideBar" }) do
	if not classes[n] then vgui.Register(n, {}, "DPanel") end
end
for name, tbl in pairs(classes) do __glua_classes[name] = tbl end
