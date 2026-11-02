-- Pure-data GLua classes: math.ease, VMatrix (4x4, row-major), CEffectData (a data holder;
-- effects reach the client with the frontend).

------------------------------------------------------------------------ math.ease
math.ease = {}
local E = math.ease
local pi, sin, cos, sqrt = math.pi, math.sin, math.cos, math.sqrt
local c1, c2, c3, c4, c5 = 1.70158, 1.70158 * 1.525, 1.70158 + 1, (2 * pi) / 3, (2 * pi) / 4.5
function E.InSine(x) return 1 - cos((x * pi) / 2) end
function E.OutSine(x) return sin((x * pi) / 2) end
function E.InOutSine(x) return -(cos(pi * x) - 1) / 2 end
function E.InQuad(x) return x * x end
function E.OutQuad(x) return 1 - (1 - x) * (1 - x) end
function E.InOutQuad(x) if x < 0.5 then return 2 * x * x end return 1 - (-2 * x + 2) ^ 2 / 2 end
function E.InCubic(x) return x * x * x end
function E.OutCubic(x) return 1 - (1 - x) ^ 3 end
function E.InOutCubic(x) if x < 0.5 then return 4 * x * x * x end return 1 - (-2 * x + 2) ^ 3 / 2 end
function E.InQuart(x) return x ^ 4 end
function E.OutQuart(x) return 1 - (1 - x) ^ 4 end
function E.InOutQuart(x) if x < 0.5 then return 8 * x ^ 4 end return 1 - (-2 * x + 2) ^ 4 / 2 end
function E.InQuint(x) return x ^ 5 end
function E.OutQuint(x) return 1 - (1 - x) ^ 5 end
function E.InOutQuint(x) if x < 0.5 then return 16 * x ^ 5 end return 1 - (-2 * x + 2) ^ 5 / 2 end
function E.InExpo(x) if x == 0 then return 0 end return 2 ^ (10 * x - 10) end
function E.OutExpo(x) if x == 1 then return 1 end return 1 - 2 ^ (-10 * x) end
function E.InOutExpo(x)
    if x == 0 then return 0 elseif x == 1 then return 1 end
    if x < 0.5 then return 2 ^ (20 * x - 10) / 2 end
    return (2 - 2 ^ (-20 * x + 10)) / 2
end
function E.InCirc(x) return 1 - sqrt(1 - x * x) end
function E.OutCirc(x) return sqrt(1 - (x - 1) ^ 2) end
function E.InOutCirc(x)
    if x < 0.5 then return (1 - sqrt(1 - (2 * x) ^ 2)) / 2 end
    return (sqrt(1 - (-2 * x + 2) ^ 2) + 1) / 2
end
function E.InBack(x) return c3 * x ^ 3 - c1 * x * x end
function E.OutBack(x) return 1 + c3 * (x - 1) ^ 3 + c1 * (x - 1) ^ 2 end
function E.InOutBack(x)
    if x < 0.5 then return ((2 * x) ^ 2 * ((c2 + 1) * 2 * x - c2)) / 2 end
    return ((2 * x - 2) ^ 2 * ((c2 + 1) * (x * 2 - 2) + c2) + 2) / 2
end
function E.InElastic(x)
    if x == 0 then return 0 elseif x == 1 then return 1 end
    return -(2 ^ (10 * x - 10)) * sin((x * 10 - 10.75) * c4)
end
function E.OutElastic(x)
    if x == 0 then return 0 elseif x == 1 then return 1 end
    return 2 ^ (-10 * x) * sin((x * 10 - 0.75) * c4) + 1
end
function E.InOutElastic(x)
    if x == 0 then return 0 elseif x == 1 then return 1 end
    if x < 0.5 then return -(2 ^ (20 * x - 10) * sin((20 * x - 11.125) * c5)) / 2 end
    return (2 ^ (-20 * x + 10) * sin((20 * x - 11.125) * c5)) / 2 + 1
end
function E.OutBounce(x)
    local n1, d1 = 7.5625, 2.75
    if x < 1 / d1 then return n1 * x * x
    elseif x < 2 / d1 then x -= 1.5 / d1; return n1 * x * x + 0.75
    elseif x < 2.5 / d1 then x -= 2.25 / d1; return n1 * x * x + 0.9375 end
    x -= 2.625 / d1
    return n1 * x * x + 0.984375
end
function E.InBounce(x) return 1 - E.OutBounce(1 - x) end
function E.InOutBounce(x) if x < 0.5 then return (1 - E.OutBounce(1 - 2 * x)) / 2 end return (1 + E.OutBounce(2 * x - 1)) / 2 end

------------------------------------------------------------------------ VMatrix
local MAT = {}
MAT.__index = MAT
local function newmat(rows) return setmetatable({ m = rows }, MAT) end
local function identity() return { { 1, 0, 0, 0 }, { 0, 1, 0, 0 }, { 0, 0, 1, 0 }, { 0, 0, 0, 1 } } end
function Matrix(data)
    if data == nil then return newmat(identity()) end
    if getmetatable(data) == MAT then return newmat(table.Copy(data.m)) end
    local rows = identity()
    for r = 1, 4 do for c = 1, 4 do if data[r] and data[r][c] ~= nil then rows[r][c] = data[r][c] end end end
    return newmat(rows)
end
MAT.__tostring = function(a)
    local out = {}
    for r = 1, 4 do out[r] = string.format("[%f %f %f %f]", a.m[r][1], a.m[r][2], a.m[r][3], a.m[r][4]) end
    return table.concat(out, "\n")
end
MAT.__eq = function(a, b) for r = 1, 4 do for c = 1, 4 do if a.m[r][c] ~= b.m[r][c] then return false end end end return true end
local function mul(a, b)
    local o = identity()
    for r = 1, 4 do for c = 1, 4 do
        local s = 0
        for k = 1, 4 do s += a[r][k] * b[k][c] end
        o[r][c] = s
    end end
    return o
end
MAT.__mul = function(a, b)
    if getmetatable(b) == MAT then return newmat(mul(a.m, b.m)) end
    -- a Vector: transform the point
    local m = a.m
    return Vector(m[1][1] * b.x + m[1][2] * b.y + m[1][3] * b.z + m[1][4], m[2][1] * b.x + m[2][2] * b.y + m[2][3] * b.z + m[2][4], m[3][1] * b.x + m[3][2] * b.y + m[3][3] * b.z + m[3][4])
end
MAT.__add = function(a, b) local o = identity(); for r = 1, 4 do for c = 1, 4 do o[r][c] = a.m[r][c] + b.m[r][c] end end return newmat(o) end
MAT.__sub = function(a, b) local o = identity(); for r = 1, 4 do for c = 1, 4 do o[r][c] = a.m[r][c] - b.m[r][c] end end return newmat(o) end
function MAT:ToTable() return table.Copy(self.m) end
function MAT:GetField(r, c) return self.m[r][c] end
function MAT:SetField(r, c, v) self.m[r][c] = v end
function MAT:Set(o) self.m = table.Copy(o.m) end
function MAT:Identity() self.m = identity() end
function MAT:IsIdentity() return self == newmat(identity()) end
function MAT:IsZero() for r = 1, 4 do for c = 1, 4 do if self.m[r][c] ~= 0 then return false end end end return true end
function MAT:Zero() for r = 1, 4 do for c = 1, 4 do self.m[r][c] = 0 end end end
function MAT:Unpack() local o = {}; for r = 1, 4 do for c = 1, 4 do o[#o + 1] = self.m[r][c] end end return unpack(o) end
function MAT:SetUnpacked(...) local v = { ... }; local i = 0; for r = 1, 4 do for c = 1, 4 do i += 1; self.m[r][c] = v[i] or 0 end end end
function MAT:GetTranslation() return Vector(self.m[1][4], self.m[2][4], self.m[3][4]) end
function MAT:SetTranslation(v) self.m[1][4], self.m[2][4], self.m[3][4] = v.x, v.y, v.z end
function MAT:Translate(v) local t = identity(); t[1][4], t[2][4], t[3][4] = v.x, v.y, v.z; self.m = mul(self.m, t) end
function MAT:GetForward() return Vector(self.m[1][1], self.m[2][1], self.m[3][1]) end
function MAT:GetRight() return Vector(-self.m[1][2], -self.m[2][2], -self.m[3][2]) end
function MAT:GetUp() return Vector(self.m[1][3], self.m[2][3], self.m[3][3]) end
function MAT:SetForward(v) self.m[1][1], self.m[2][1], self.m[3][1] = v.x, v.y, v.z end
function MAT:SetRight(v) self.m[1][2], self.m[2][2], self.m[3][2] = -v.x, -v.y, -v.z end
function MAT:SetUp(v) self.m[1][3], self.m[2][3], self.m[3][3] = v.x, v.y, v.z end
function MAT:GetScale() return Vector(self:GetForward():Length(), Vector(self.m[1][2], self.m[2][2], self.m[3][2]):Length(), self:GetUp():Length()) end
function MAT:SetScale(s)
    local f, l, u = self:GetForward():GetNormalized(), Vector(self.m[1][2], self.m[2][2], self.m[3][2]):GetNormalized(), self:GetUp():GetNormalized()
    self:SetForward(f * s.x); self.m[1][2], self.m[2][2], self.m[3][2] = l.x * s.y, l.y * s.y, l.z * s.y; self:SetUp(u * s.z)
end
function MAT:Scale(s) local t = identity(); t[1][1], t[2][2], t[3][3] = s.x, s.y, s.z; self.m = mul(self.m, t) end
function MAT:ScaleTranslation(k) self.m[1][4] *= k; self.m[2][4] *= k; self.m[3][4] *= k end
function MAT:GetAngles() return self:GetForward():AngleEx(self:GetUp()) end
function MAT:SetAngles(a)
    local f, r, u = a:Forward(), a:Right(), a:Up()
    local s = self:GetScale()
    self:SetForward(f * s.x); self.m[1][2], self.m[2][2], self.m[3][2] = -r.x * s.y, -r.y * s.y, -r.z * s.y; self:SetUp(u * s.z)
end
function MAT:Rotate(a) local t = Matrix(); t:SetAngles(a); self.m = mul(self.m, t.m) end
function MAT:Transpose() local o = identity(); for r = 1, 4 do for c = 1, 4 do o[r][c] = self.m[c][r] end end self.m = o end
function MAT:GetTransposed() local o = Matrix(self); o:Transpose(); return o end
function MAT:IsRotationMatrix()
    local f, l, u = self:GetForward(), Vector(self.m[1][2], self.m[2][2], self.m[3][2]), self:GetUp()
    return math.abs(f:Length() - 1) < 1e-4 and math.abs(l:Length() - 1) < 1e-4 and math.abs(u:Length() - 1) < 1e-4 and math.abs(f:Dot(l)) < 1e-4 and math.abs(f:Dot(u)) < 1e-4 and math.abs(l:Dot(u)) < 1e-4
end
function MAT:InvertTR()
    if not self:IsRotationMatrix() then return false end
    local m = self.m
    local t = self:GetTranslation()
    local o = identity()
    for r = 1, 3 do for c = 1, 3 do o[r][c] = m[c][r] end end
    o[1][4] = -(o[1][1] * t.x + o[1][2] * t.y + o[1][3] * t.z)
    o[2][4] = -(o[2][1] * t.x + o[2][2] * t.y + o[2][3] * t.z)
    o[3][4] = -(o[3][1] * t.x + o[3][2] * t.y + o[3][3] * t.z)
    self.m = o
    return true
end
function MAT:GetInverseTR() local o = Matrix(self); o:InvertTR(); return o end
function MAT:Invert()
    -- Gauss-Jordan on the 4x4
    local a = table.Copy(self.m)
    local inv = identity()
    for i = 1, 4 do
        local piv = i
        for r = i + 1, 4 do if math.abs(a[r][i]) > math.abs(a[piv][i]) then piv = r end end
        if math.abs(a[piv][i]) < 1e-12 then return false end
        a[i], a[piv] = a[piv], a[i]; inv[i], inv[piv] = inv[piv], inv[i]
        local d = a[i][i]
        for c = 1, 4 do a[i][c] /= d; inv[i][c] /= d end
        for r = 1, 4 do if r ~= i then
            local f = a[r][i]
            for c = 1, 4 do a[r][c] -= f * a[i][c]; inv[r][c] -= f * inv[i][c] end
        end end
    end
    self.m = inv
    return true
end
function MAT:GetInverse() local o = Matrix(self); if o:Invert() then return o end return nil end
function MAT:Mul(o) self.m = mul(self.m, o.m) end
function MAT:Add(o) for r = 1, 4 do for c = 1, 4 do self.m[r][c] += o.m[r][c] end end end
function MAT:Sub(o) for r = 1, 4 do for c = 1, 4 do self.m[r][c] -= o.m[r][c] end end end
__glua_classes.VMatrix = MAT

------------------------------------------------------------------------ CEffectData
local EFF = {}
EFF.__index = EFF
function EffectData()
    return setmetatable({ origin = Vector(), start = Vector(), normal = Vector(), angles = Angle(), ent = NULL, attachment = 0, color = 0, damage_type = 0, entindex = 0, flags = 0, hitbox = 0, magnitude = 0, material = 0, radius = 0, scale = 0, surface = 0 }, EFF)
end
for _, k in ipairs({ { "Origin", "origin" }, { "Start", "start" }, { "Normal", "normal" }, { "Angles", "angles" }, { "Entity", "ent" }, { "Attachment", "attachment" }, { "ColorR", "color" }, { "DamageType", "damage_type" }, { "EntIndex", "entindex" }, { "Flags", "flags" }, { "HitBox", "hitbox" }, { "Magnitude", "magnitude" }, { "MaterialIndex", "material" }, { "Radius", "radius" }, { "Scale", "scale" }, { "SurfaceProp", "surface" } }) do
    EFF["Get" .. k[1]] = function(self) return self[k[2]] end
    EFF["Set" .. k[1]] = function(self, v) self[k[2]] = v end
end
function EFF:GetColor() return self.color end
function EFF:SetColor(c) self.color = c end
function EFF:GetEntityIndex() return IsValid(self.ent) and self.ent:EntIndex() or self.entindex end
__glua_classes.CEffectData = EFF
local effects_sent = {}
function util.Effect(name, data, allowOverride, ignorePredictionOrRecipientFilter) effects_sent[#effects_sent + 1] = { name, data } end
effects = effects or {}
local effect_defs = {}
function effects.Register(def, name) effect_defs[name] = def end
function effects.Create(name) return effect_defs[name] end
function effects.BeamRingPoint() end
function effects.Bubbles() end
function effects.BubbleTrail() end
