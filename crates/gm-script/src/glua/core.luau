-- GLua core surface (own implementation from the documented behaviour): Vector, Angle, Color,
-- math / string / table / bit / utf8 extensions, hook and timer libraries.
local floor, ceil, sqrt, abs, sin, cos, atan2, pi = math.floor, math.ceil, math.sqrt, math.abs, math.sin, math.cos, math.atan2, math.pi
local rad, deg = math.rad, math.deg

------------------------------------------------------------------------ Vector
local VEC = {}
VEC.__index = VEC
local function isvec(v) return type(v) == "table" and getmetatable(v) == VEC end
local function newvec(x, y, z) return setmetatable({ x = x or 0, y = y or 0, z = z or 0 }, VEC) end
function Vector(x, y, z)
    if type(x) == "string" then
        local a, b, c = x:match("([%-%d%.e]+)%s+([%-%d%.e]+)%s+([%-%d%.e]+)")
        return newvec(tonumber(a) or 0, tonumber(b) or 0, tonumber(c) or 0)
    elseif type(x) == "table" then
        return newvec(x.x, x.y, x.z)
    end
    return newvec(x, y, z)
end
vector_origin = newvec(0, 0, 0)
vector_up = newvec(0, 0, 1)
VEC.__tostring = function(v) return string.format("%f %f %f", v.x, v.y, v.z) end
VEC.__eq = function(a, b) return a.x == b.x and a.y == b.y and a.z == b.z end
VEC.__add = function(a, b) return newvec(a.x + b.x, a.y + b.y, a.z + b.z) end
VEC.__sub = function(a, b) return newvec(a.x - b.x, a.y - b.y, a.z - b.z) end
VEC.__unm = function(a) return newvec(-a.x, -a.y, -a.z) end
VEC.__mul = function(a, b)
    if type(a) == "number" then return newvec(a * b.x, a * b.y, a * b.z) end
    if type(b) == "number" then return newvec(a.x * b, a.y * b, a.z * b) end
    return newvec(a.x * b.x, a.y * b.y, a.z * b.z)
end
VEC.__div = function(a, b)
    if type(b) == "number" then return newvec(a.x / b, a.y / b, a.z / b) end
    return newvec(a.x / b.x, a.y / b.y, a.z / b.z)
end
function VEC:Add(o) self.x += o.x; self.y += o.y; self.z += o.z end
function VEC:Sub(o) self.x -= o.x; self.y -= o.y; self.z -= o.z end
function VEC:Mul(m)
    if type(m) == "number" then self.x *= m; self.y *= m; self.z *= m
    elseif isvec(m) then self.x *= m.x; self.y *= m.y; self.z *= m.z
    elseif type(m) == "table" and m.GetTranslation then local r = m * self; self.x, self.y, self.z = r.x, r.y, r.z end
end
function VEC:Div(d) self.x /= d; self.y /= d; self.z /= d end
function VEC:Negate() self.x, self.y, self.z = -self.x, -self.y, -self.z end
function VEC:GetNegated() return newvec(-self.x, -self.y, -self.z) end
function VEC:Dot(o) return self.x * o.x + self.y * o.y + self.z * o.z end
VEC.DotProduct = VEC.Dot
function VEC:Cross(o) return newvec(self.y * o.z - self.z * o.y, self.z * o.x - self.x * o.z, self.x * o.y - self.y * o.x) end
function VEC:Length() return sqrt(self.x * self.x + self.y * self.y + self.z * self.z) end
function VEC:LengthSqr() return self.x * self.x + self.y * self.y + self.z * self.z end
function VEC:Length2D() return sqrt(self.x * self.x + self.y * self.y) end
function VEC:Length2DSqr() return self.x * self.x + self.y * self.y end
function VEC:Distance(o) local dx, dy, dz = self.x - o.x, self.y - o.y, self.z - o.z; return sqrt(dx * dx + dy * dy + dz * dz) end
function VEC:DistToSqr(o) local dx, dy, dz = self.x - o.x, self.y - o.y, self.z - o.z; return dx * dx + dy * dy + dz * dz end
function VEC:Distance2D(o) local dx, dy = self.x - o.x, self.y - o.y; return sqrt(dx * dx + dy * dy) end
function VEC:Distance2DSqr(o) local dx, dy = self.x - o.x, self.y - o.y; return dx * dx + dy * dy end
function VEC:GetNormalized()
    local l = self:Length()
    if l < 1e-12 then return newvec(0, 0, 0) end
    return newvec(self.x / l, self.y / l, self.z / l)
end
VEC.GetNormal = VEC.GetNormalized
function VEC:Normalize()
    local l = self:Length()
    if l < 1e-12 then return end
    self.x /= l; self.y /= l; self.z /= l
end
function VEC:IsZero() return self.x == 0 and self.y == 0 and self.z == 0 end
function VEC:Zero() self.x, self.y, self.z = 0, 0, 0 end
function VEC:Set(o) self.x, self.y, self.z = o.x, o.y, o.z end
function VEC:SetUnpacked(x, y, z) self.x, self.y, self.z = x, y, z end
function VEC:Unpack() return self.x, self.y, self.z end
function VEC:ToTable() return { self.x, self.y, self.z } end
function VEC:IsEqualTol(o, tol)
    return abs(self.x - o.x) <= tol and abs(self.y - o.y) <= tol and abs(self.z - o.z) <= tol
end
function VEC:WithinAABox(a, b)
    local function within(v, lo, hi) if lo > hi then lo, hi = hi, lo end return v >= lo and v <= hi end
    return within(self.x, a.x, b.x) and within(self.y, a.y, b.y) and within(self.z, a.z, b.z)
end
function VEC:Random(min, max)
    min = min or -1; max = max or 1
    self.x = min + math.random() * (max - min)
    self.y = min + math.random() * (max - min)
    self.z = min + math.random() * (max - min)
end
function VEC:ToColor() return Color(self.x * 255, self.y * 255, self.z * 255) end
function VEC:Angle()
    -- Source's VectorAngles: pitch from the vertical, yaw from +x; a zero vector gives 0 0 0
    if self.x == 0 and self.y == 0 then
        if self.z == 0 then return Angle(0, 0, 0) end
        return Angle(self.z > 0 and 270 or 90, 0, 0)
    end
    local yaw = deg(atan2(self.y, self.x)); if yaw < 0 then yaw += 360 end
    local pitch = deg(atan2(-self.z, self:Length2D())); if pitch < 0 then pitch += 360 end
    if yaw == 0 then yaw = 0 end
    if pitch == 0 then pitch = 0 end
    return Angle(pitch, yaw, 0)
end
function VEC:AngleEx(up)
    local a = self:Angle()
    -- roll: the angle between the requested up and the forward-derived up around the forward axis
    local f = self:GetNormalized()
    local left = up:Cross(f):GetNormalized()
    local u = f:Cross(left)
    a.r = deg(atan2(left.z, u.z))
    return a
end
function VEC:Rotate(ang)
    local f, r, u = ang:Forward(), ang:Right(), ang:Up()
    local x = self.x * f.x - self.y * r.x + self.z * u.x
    local y = self.x * f.y - self.y * r.y + self.z * u.y
    local z = self.x * f.z - self.y * r.z + self.z * u.z
    self.x, self.y, self.z = x, y, z
end

------------------------------------------------------------------------ Angle
local ANG = {}
ANG.__index = ANG
local function newang(p, y, r) return setmetatable({ p = p or 0, y = y or 0, r = r or 0 }, ANG) end
function Angle(p, y, r)
    if type(p) == "string" then
        local a, b, c = p:match("([%-%d%.e]+)%s+([%-%d%.e]+)%s+([%-%d%.e]+)")
        return newang(tonumber(a) or 0, tonumber(b) or 0, tonumber(c) or 0)
    elseif type(p) == "table" then
        return newang(p.p, p.y, p.r)
    end
    return newang(p, y, r)
end
angle_zero = newang(0, 0, 0)
ANG.__tostring = function(a) return string.format("%f %f %f", a.p, a.y, a.r) end
ANG.__eq = function(a, b) return a.p == b.p and a.y == b.y and a.r == b.r end
ANG.__add = function(a, b) return newang(a.p + b.p, a.y + b.y, a.r + b.r) end
ANG.__sub = function(a, b) return newang(a.p - b.p, a.y - b.y, a.r - b.r) end
ANG.__unm = function(a) return newang(-a.p, -a.y, -a.r) end
ANG.__mul = function(a, b)
    if type(a) == "number" then return newang(a * b.p, a * b.y, a * b.r) end
    return newang(a.p * b, a.y * b, a.r * b)
end
ANG.__div = function(a, b) return newang(a.p / b, a.y / b, a.r / b) end
function ANG:Add(o) self.p += o.p; self.y += o.y; self.r += o.r end
function ANG:Sub(o) self.p -= o.p; self.y -= o.y; self.r -= o.r end
function ANG:Mul(s) self.p *= s; self.y *= s; self.r *= s end
function ANG:Div(s) self.p /= s; self.y /= s; self.r /= s end
function ANG:Set(o) self.p, self.y, self.r = o.p, o.y, o.r end
function ANG:SetUnpacked(p, y, r) self.p, self.y, self.r = p, y, r end
function ANG:Unpack() return self.p, self.y, self.r end
function ANG:ToTable() return { self.p, self.y, self.r } end
function ANG:IsZero() return self.p == 0 and self.y == 0 and self.r == 0 end
function ANG:Zero() self.p, self.y, self.r = 0, 0, 0 end
function ANG:IsEqualTol(o, tol)
    return abs(self.p - o.p) <= tol and abs(self.y - o.y) <= tol and abs(self.r - o.r) <= tol
end
local function normang(a) a = a % 360; if a > 180 then a -= 360 end return a end
function ANG:Normalize() self.p, self.y, self.r = normang(self.p), normang(self.y), normang(self.r) end
function ANG:Random(min, max)
    min = min or -1; max = max or 1
    self.p = min + math.random() * (max - min)
    self.y = min + math.random() * (max - min)
    self.r = min + math.random() * (max - min)
end
function ANG:Forward()
    local sp, cp, sy, cy = sin(rad(self.p)), cos(rad(self.p)), sin(rad(self.y)), cos(rad(self.y))
    return newvec(cp * cy, cp * sy, -sp)
end
function ANG:Right()
    local sp, cp = sin(rad(self.p)), cos(rad(self.p))
    local sy, cy = sin(rad(self.y)), cos(rad(self.y))
    local sr, cr = sin(rad(self.r)), cos(rad(self.r))
    return newvec(-1 * sr * sp * cy + -1 * cr * -sy, -1 * sr * sp * sy + -1 * cr * cy, -1 * sr * cp)
end
function ANG:Up()
    local sp, cp = sin(rad(self.p)), cos(rad(self.p))
    local sy, cy = sin(rad(self.y)), cos(rad(self.y))
    local sr, cr = sin(rad(self.r)), cos(rad(self.r))
    return newvec(cr * sp * cy + -sr * -sy, cr * sp * sy + -sr * cy, cr * cp)
end
function ANG:RotateAroundAxis(axis, rot)
    -- rotate the basis vectors with Rodrigues' formula and rebuild the angle
    local k = axis:GetNormalized()
    local c, s = cos(rad(rot)), sin(rad(rot))
    local function rot3(v)
        return v * c + k:Cross(v) * s + k * (k:Dot(v) * (1 - c))
    end
    local f, u = rot3(self:Forward()), rot3(self:Up())
    local a = f:AngleEx(u)
    self.p, self.y, self.r = a.p, a.y, a.r
end
function ANG:SnapTo(axis, target)
    local v = self[axis]
    local snapped = math.Round(v / target) * target
    local a = newang(self.p, self.y, self.r)
    a[axis] = normang(snapped)
    return a
end

------------------------------------------------------------------------ Color
local COL = {}
COL.__index = COL
local function newcol(r, g, b, a) return setmetatable({ r = r or 255, g = g or 255, b = b or 255, a = a or 255 }, COL) end
function Color(r, g, b, a) return newcol(r, g, b, a) end
function ColorAlpha(c, a) return newcol(c.r, c.g, c.b, a) end
function IsColor(v) return type(v) == "table" and getmetatable(v) == COL end
function ColorRand(alpha) return newcol(math.random(0, 255), math.random(0, 255), math.random(0, 255), alpha and math.random(0, 255) or 255) end
function HSVToColor(h, s, v)
    h = h % 360
    local c = v * s
    local x = c * (1 - abs((h / 60) % 2 - 1))
    local m = v - c
    local r, g, b
    if h < 60 then r, g, b = c, x, 0 elseif h < 120 then r, g, b = x, c, 0 elseif h < 180 then r, g, b = 0, c, x
    elseif h < 240 then r, g, b = 0, x, c elseif h < 300 then r, g, b = x, 0, c else r, g, b = c, 0, x end
    return newcol(floor((r + m) * 255 + 0.5), floor((g + m) * 255 + 0.5), floor((b + m) * 255 + 0.5), 255)
end
function HSLToColor(h, s, l)
    local c = (1 - abs(2 * l - 1)) * s
    local v = l + c / 2
    return HSVToColor(h, v == 0 and 0 or c / v, v)
end
function ColorToHSV(c)
    local r, g, b = c.r / 255, c.g / 255, c.b / 255
    local mx, mn = math.max(r, g, b), math.min(r, g, b)
    local d = mx - mn
    local h = 0
    if d > 0 then
        if mx == r then h = 60 * (((g - b) / d) % 6)
        elseif mx == g then h = 60 * ((b - r) / d + 2)
        else h = 60 * ((r - g) / d + 4) end
    end
    return h, mx == 0 and 0 or d / mx, mx
end
function ColorToHSL(c)
    local h, s, v = ColorToHSV(c)
    local l = v * (1 - s / 2)
    local sl = (l == 0 or l == 1) and 0 or (v - l) / math.min(l, 1 - l)
    return h, sl, l
end
COL.__tostring = function(c) return c.r .. " " .. c.g .. " " .. c.b .. " " .. c.a end
COL.__eq = function(a, b) return a.r == b.r and a.g == b.g and a.b == b.b and a.a == b.a end
function COL:Copy() return newcol(self.r, self.g, self.b, self.a) end
function COL:SetUnpacked(r, g, b, a) self.r, self.g, self.b, self.a = r, g, b, a end
function COL:Unpack() return self.r, self.g, self.b, self.a end
function COL:ToTable() return { self.r, self.g, self.b, self.a } end
function COL:ToVector() return newvec(self.r / 255, self.g / 255, self.b / 255) end
function COL:ToHSV() return ColorToHSV(self) end
function COL:ToHSL() return ColorToHSL(self) end
function COL:ToHWB()
    local h, s, v = ColorToHSV(self)
    return h, (1 - s) * v, 1 - v
end
function COL:ToHex(alpha)
    if alpha then return string.format("#%02X%02X%02X%02X", self.r, self.g, self.b, self.a) end
    return string.format("#%02X%02X%02X", self.r, self.g, self.b)
end
function COL:Lerp(target, frac)
    return newcol(math.Lerp(frac, self.r, target.r), math.Lerp(frac, self.g, target.g), math.Lerp(frac, self.b, target.b), math.Lerp(frac, self.a, target.a))
end
local function setfromhsv(c, h, s, v) local n = HSVToColor(h, s, v); c.r, c.g, c.b = n.r, n.g, n.b end
function COL:GetHue() return (ColorToHSV(self)) end
function COL:GetSaturation() local _, s = ColorToHSV(self); return s end
function COL:GetBrightness() local _, _, v = ColorToHSV(self); return v end
function COL:GetLightness() local _, _, l = ColorToHSL(self); return l end
function COL:GetWhiteness() local _, w = self:ToHWB(); return w end
function COL:GetBlackness() local _, _, b = self:ToHWB(); return b end
function COL:SetHue(h) local _, s, v = ColorToHSV(self); setfromhsv(self, h, s, v) end
function COL:SetSaturation(s) local h, _, v = ColorToHSV(self); setfromhsv(self, h, math.Clamp(s, 0, 1), v) end
function COL:SetBrightness(v) local h, s = ColorToHSV(self); setfromhsv(self, h, s, math.Clamp(v, 0, 1)) end
function COL:SetLightness(l)
    local h, s = ColorToHSL(self)
    local n = HSLToColor(h, s, math.Clamp(l, 0, 1)); self.r, self.g, self.b = n.r, n.g, n.b
end
local function setfromhwb(c, h, w, b)
    if w + b > 1 then local t = w + b; w, b = w / t, b / t end
    local v = 1 - b
    setfromhsv(c, h, v == 0 and 0 or 1 - w / v, v)
end
function COL:SetWhiteness(w) local h, _, b = self:ToHWB(); setfromhwb(self, h, math.Clamp(w, 0, 1), b) end
function COL:SetBlackness(b) local h, w = self:ToHWB(); setfromhwb(self, h, w, math.Clamp(b, 0, 1)) end
function COL:AddHue(d) self:SetHue((self:GetHue() + d) % 360) end
function COL:AddSaturation(d) self:SetSaturation(self:GetSaturation() + d) end
function COL:AddBrightness(d) self:SetBrightness(self:GetBrightness() + d) end
function COL:AddLightness(d) self:SetLightness(self:GetLightness() + d) end
function COL:AddWhiteness(d) self:SetWhiteness(self:GetWhiteness() + d) end
function COL:AddBlackness(d) self:SetBlackness(self:GetBlackness() + d) end

------------------------------------------------------------------------ math
math.tau = 2 * pi
math.mod = math.fmod
math.pow = math.pow or function(a, b) return a ^ b end
math.log10 = math.log10 or function(x) return math.log(x, 10) end
math.cosh = math.cosh or function(x) return (math.exp(x) + math.exp(-x)) / 2 end
math.sinh = math.sinh or function(x) return (math.exp(x) - math.exp(-x)) / 2 end
math.tanh = math.tanh or function(x) local e = math.exp(2 * x); return (e - 1) / (e + 1) end
function math.Clamp(v, lo, hi) if v < lo then return lo elseif v > hi then return hi end return v end
function math.Round(v, decimals)
    local m = 10 ^ (decimals or 0)
    return floor(v * m + 0.5) / m
end
function math.Truncate(v, decimals)
    local m = 10 ^ (decimals or 0)
    return (v >= 0 and floor(v * m) or ceil(v * m)) / m
end
function math.Approach(cur, target, inc)
    inc = abs(inc)
    if cur < target then return math.min(cur + inc, target) elseif cur > target then return math.max(cur - inc, target) end
    return target
end
function math.NormalizeAngle(a) return normang(a) end
function math.AngleDifference(a, b) return normang(a - b) end
function math.ApproachAngle(cur, target, inc) return math.Approach(cur, cur + math.AngleDifference(target, cur), inc) end
function math.Dist(x1, y1, x2, y2) local dx, dy = x2 - x1, y2 - y1; return sqrt(dx * dx + dy * dy) end
math.Distance = math.Dist
function math.DistanceSqr(x1, y1, x2, y2) local dx, dy = x2 - x1, y2 - y1; return dx * dx + dy * dy end
function math.Rand(lo, hi) return lo + math.random() * (hi - lo) end
function math.Remap(v, inMin, inMax, outMin, outMax) return outMin + (outMax - outMin) * ((v - inMin) / (inMax - inMin)) end
function math.Sign(v) if v > 0 then return 1 elseif v < 0 then return -1 end return 0 end
function math.SnapTo(v, step) return math.Round(v / step) * step end
function math.TimeFraction(start, finish, cur) return (cur - start) / (finish - start) end
function math.EaseInOut(frac, easeIn, easeOut)
    easeIn = easeIn or 0.5; easeOut = easeOut or 0.5
    if frac <= 0 then return 0 elseif frac >= 1 then return 1 end
    local sumEase = easeIn + easeOut
    if sumEase <= 0 then return frac end
    if sumEase > 1 then easeIn /= sumEase; easeOut /= sumEase end
    local area = 1 - (easeIn + easeOut) / 2
    if frac < easeIn then return frac * frac / (2 * easeIn) / area end
    if frac > 1 - easeOut then local t = 1 - frac; return (1 - t * t / (2 * easeOut) / area) end
    return (frac - easeIn / 2) / area
end
function math.Factorial(n) local r = 1; for i = 2, n do r *= i end return r end
function math.IsNearlyEqual(a, b, tol) return abs(a - b) <= (tol or 1e-8) end
function math.BinToInt(s) return tonumber(s, 2) end
function math.IntToBin(n)
    n = floor(n)
    if n == 0 then return "0" end
    local t = {}
    while n > 0 do table.insert(t, 1, tostring(n % 2)); n = floor(n / 2) end
    return table.concat(t)
end
function math.QuadraticBezier(frac, p0, p1, p2)
    local a = 1 - frac
    return p0 * (a * a) + p1 * (2 * a * frac) + p2 * (frac * frac)
end
function math.CubicBezier(frac, p0, p1, p2, p3)
    local a = 1 - frac
    return p0 * (a * a * a) + p1 * (3 * a * a * frac) + p2 * (3 * a * frac * frac) + p3 * (frac * frac * frac)
end
function math.CHSpline(frac, p0, p1, p2, p3)
    local t2, t3 = frac * frac, frac * frac * frac
    return p1 * (2 * t3 - 3 * t2 + 1) + p2 * (-2 * t3 + 3 * t2) + (p2 - p0) * (0.5 * (t3 - 2 * t2 + frac)) + (p3 - p1) * (0.5 * (t3 - t2))
end
function math.calcBSplineN(i, k, t, tinc)
    if k == 1 then
        if t >= i * tinc and t < (i + 1) * tinc then return 1 end
        return 0
    end
    local a = (t - i * tinc) / ((k - 1) * tinc) * math.calcBSplineN(i, k - 1, t, tinc)
    local b = ((i + k) * tinc - t) / ((k - 1) * tinc) * math.calcBSplineN(i + 1, k - 1, t, tinc)
    return a + b
end
function math.BSplinePoint(tDiff, tPoints, tMax)
    local n = #tPoints
    local tinc = tMax / (n - 3)
    local t = tDiff + 3 * tinc
    local p = Vector(0, 0, 0)
    for i = 1, n do
        local w = math.calcBSplineN(i - 1, 4, t, tinc)
        if w ~= 0 then p += tPoints[i] * w end
    end
    return p
end
function Lerp(frac, from, to) return from + (to - from) * frac end
math.Lerp = Lerp
function LerpVector(frac, from, to) return newvec(Lerp(frac, from.x, to.x), Lerp(frac, from.y, to.y), Lerp(frac, from.z, to.z)) end
function LerpAngle(frac, from, to)
    return newang(from.p + normang(to.p - from.p) * frac, from.y + normang(to.y - from.y) * frac, from.r + normang(to.r - from.r) * frac)
end

------------------------------------------------------------------------ string
local smeta = getmetatable("")
string.gfind = string.gmatch
function string.Left(s, n) return string.sub(s, 1, n) end
function string.Right(s, n) return string.sub(s, -n) end
function string.GetChar(s, i) return string.sub(s, i, i) end
function string.SetChar(s, i, c) return string.sub(s, 1, i - 1) .. c .. string.sub(s, i + 1) end
function string.StartsWith(s, p) return string.sub(s, 1, #p) == p end
string.StartWith = string.StartsWith
function string.EndsWith(s, p) return p == "" or string.sub(s, -#p) == p end
function string.PatternSafe(s) return (string.gsub(s, "[%(%)%.%%%+%-%*%?%[%]%^%$%z]", "%%%0")) end
function string.Replace(s, find, rep)
    local out, i = {}, 1
    while true do
        local a, b = string.find(s, find, i, true)
        if not a then break end
        out[#out + 1] = string.sub(s, i, a - 1)
        out[#out + 1] = rep
        i = b + 1
        if b < a then out[#out + 1] = string.sub(s, a, a); i = a + 1 end
    end
    out[#out + 1] = string.sub(s, i)
    return table.concat(out)
end
function string.Trim(s, c) c = c and string.PatternSafe(c) or "%s"; return (string.gsub(s, "^" .. c .. "*(.-)" .. c .. "*$", "%1")) end
function string.TrimLeft(s, c) c = c and string.PatternSafe(c) or "%s"; return (string.gsub(s, "^" .. c .. "*", "")) end
function string.TrimRight(s, c) c = c and string.PatternSafe(c) or "%s"; return (string.gsub(s, c .. "*$", "")) end
function string.Explode(sep, s, pattern)
    if sep == "" then return string.ToTable(s) end
    local out, i = {}, 1
    while true do
        local a, b = string.find(s, sep, i, not pattern)
        if not a then break end
        out[#out + 1] = string.sub(s, i, a - 1)
        i = b + 1
    end
    out[#out + 1] = string.sub(s, i)
    return out
end
function string.Split(s, sep) return string.Explode(sep, s, false) end
function string.Implode(sep, t) return table.concat(t, sep or "") end
function string.ToTable(s)
    local t = {}
    for i = 1, #s do t[i] = string.sub(s, i, i) end
    return t
end
function string.Comma(n, sep)
    sep = sep or ","
    local s = tostring(n)
    local int, frac = string.match(s, "^(-?%d+)(.*)$")
    if not int then return s end
    local out = string.gsub(string.reverse(int), "(%d%d%d)", "%1" .. sep)
    out = string.reverse(out)
    out = string.gsub(out, "^" .. string.PatternSafe(sep), "")
    out = string.gsub(out, "^%-" .. string.PatternSafe(sep), "-")
    return out .. frac
end
function string.NiceSize(bytes)
    if bytes < 1000 then return bytes .. " Bytes" end
    if bytes < 1000 * 1000 then return math.Round(bytes / 1000, 2) .. " KB" end
    if bytes < 1000 * 1000 * 1000 then return math.Round(bytes / (1000 * 1000), 2) .. " MB" end
    return math.Round(bytes / (1000 * 1000 * 1000), 2) .. " GB"
end
function string.NiceTime(seconds)
    if seconds == nil then return "a few seconds" end
    if seconds < 60 then local t = floor(seconds); return t .. (t == 1 and " second" or " seconds") end
    if seconds < 3600 then local t = floor(seconds / 60); return t .. (t == 1 and " minute" or " minutes") end
    if seconds < 86400 then local t = floor(seconds / 3600); return t .. (t == 1 and " hour" or " hours") end
    if seconds < 604800 then local t = floor(seconds / 86400); return t .. (t == 1 and " day" or " days") end
    if seconds < 31536000 then local t = floor(seconds / 604800); return t .. (t == 1 and " week" or " weeks") end
    local t = floor(seconds / 31536000); return t .. (t == 1 and " year" or " years")
end
function string.FormattedTime(seconds, fmt)
    seconds = seconds or 0
    local h = floor(seconds / 3600)
    local m = floor(seconds / 60) % 60
    local s = floor(seconds % 60)
    local ms = floor((seconds - floor(seconds)) * 100)
    if fmt then return string.format(fmt, m, s, ms) end
    return { h = h, m = m, s = s, ms = ms }
end
function string.ToMinutesSeconds(seconds) return string.FormattedTime(seconds, "%02i:%02i") end
function string.ToMinutesSecondsMilliseconds(seconds) return string.FormattedTime(seconds, "%02i:%02i:%02i") end
function string.NiceName(s)
    local name = s
    for pre in string.gmatch(s, "^.-_") do name = string.sub(s, #pre + 1); break end
    return (string.gsub(name, "^%l", string.upper))
end
function string.CardinalToOrdinal(n)
    local r = n % 100
    if r >= 11 and r <= 13 then return n .. "th" end
    r = n % 10
    if r == 1 then return n .. "st" elseif r == 2 then return n .. "nd" elseif r == 3 then return n .. "rd" end
    return n .. "th"
end
function string.GetExtensionFromFilename(p) return string.match(p, "%.([^%./\\]+)$") end
function string.StripExtension(p)
    local i = string.match(p, ".+()%.[^%./\\]+$")
    if i then return string.sub(p, 1, i - 1) end
    return p
end
function string.GetFileFromFilename(p)
    if not string.find(p, "[\\/]") then return p end
    return string.match(p, "[\\/]([^/\\]*)$") or ""
end
function string.GetPathFromFilename(p) return string.match(p, "^(.*[/\\])[^/\\]-$") or "" end
function string.FromColor(c) return string.format("%d %d %d %d", c.r, c.g, c.b, c.a) end
function string.ToColor(s)
    local r, g, b, a = string.match(s, "([%d%.]+)%s+([%d%.]+)%s+([%d%.]+)%s*([%d%.]*)")
    return Color(tonumber(r) or 255, tonumber(g) or 255, tonumber(b) or 255, tonumber(a) or 255)
end
function string.JavascriptSafe(s)
    s = string.gsub(s, "\\", "\\\\")
    s = string.gsub(s, "\"", "\\\"")
    s = string.gsub(s, "'", "\\'")
    s = string.gsub(s, "\n", "\\n")
    s = string.gsub(s, "\r", "\\r")
    s = string.gsub(s, "\226\128\168", "\\\226\128\168")
    s = string.gsub(s, "\226\128\169", "\\\226\128\169")
    return s
end
function string.Interpolate(s, tbl) return (string.gsub(s, "{(.-)}", function(k) local v = tbl[k]; return v ~= nil and tostring(v) or "{" .. k .. "}" end)) end
string.dump = string.dump or function() error("string.dump: bytecode is not available") end
if smeta then
    for k, v in pairs(string) do if type(v) == "function" then smeta.__index = string end end
end

------------------------------------------------------------------------ table
table.getn = table.getn or function(t) return #t end
table.maxn = table.maxn or function(t) local m = 0; for k in pairs(t) do if type(k) == "number" and k > m then m = k end end return m end
table.foreach = table.foreach or function(t, f) for k, v in pairs(t) do local r = f(k, v); if r ~= nil then return r end end end
table.foreachi = table.foreachi or function(t, f) for i, v in ipairs(t) do local r = f(i, v); if r ~= nil then return r end end end
table.Pack = table.pack or function(...) return { n = select("#", ...), ... } end
function table.Add(dst, src) for _, v in pairs(src) do table.insert(dst, v) end return dst end
function table.Copy(t, done)
    if type(t) ~= "table" then return t end
    done = done or {}
    if done[t] then return done[t] end
    local c = {}
    done[t] = c
    setmetatable(c, getmetatable(t))
    for k, v in pairs(t) do
        c[type(k) == "table" and table.Copy(k, done) or k] = type(v) == "table" and table.Copy(v, done) or v
    end
    return c
end
function table.CopyFromTo(from, to) for k in pairs(to) do to[k] = nil end for k, v in pairs(from) do to[k] = v end end
function table.Count(t) local n = 0; for _ in pairs(t) do n += 1 end return n end
function table.Empty(t) for k in pairs(t) do t[k] = nil end end
function table.IsEmpty(t) return next(t) == nil end
function table.HasValue(t, v) for _, x in pairs(t) do if x == v then return true end end return false end
function table.KeyFromValue(t, v) for k, x in pairs(t) do if x == v then return k end end end
function table.KeysFromValue(t, v) local r = {}; for k, x in pairs(t) do if x == v then r[#r + 1] = k end end return r end
function table.GetKeys(t) local r = {}; for k in pairs(t) do r[#r + 1] = k end return r end
function table.GetFirstKey(t) local k = next(t); return k end
function table.GetFirstValue(t) local _, v = next(t); return v end
function table.GetLastKey(t) local last; for k in pairs(t) do last = k end return last end
function table.GetLastValue(t) local last; for _, v in pairs(t) do last = v end return last end
function table.Inherit(t, base)
    for k, v in pairs(base) do if t[k] == nil then t[k] = v end end
    t.BaseClass = base
    return t
end
function table.Merge(dst, src, deep)
    for k, v in pairs(src) do
        if deep and type(v) == "table" and type(dst[k]) == "table" then table.Merge(dst[k], v, true) else dst[k] = v end
    end
    return dst
end
function table.Random(t)
    local n = table.Count(t)
    if n == 0 then return nil end
    local pick = math.random(1, n)
    for k, v in pairs(t) do pick -= 1; if pick == 0 then return v, k end end
end
function table.RemoveByValue(t, v)
    for k, x in pairs(t) do
        if x == v then
            if type(k) == "number" then table.remove(t, k) else t[k] = nil end
            return k
        end
    end
    return false
end
function table.Reverse(t) local r = {}; local n = #t; for i = 1, n do r[i] = t[n - i + 1] end return r end
function table.Shuffle(t)
    for i = #t, 2, -1 do local j = math.random(1, i); t[i], t[j] = t[j], t[i] end
end
function table.IsSequential(t)
    local i = 1
    for _ in pairs(t) do if t[i] == nil then return false end i += 1 end
    return true
end
function table.ClearKeys(t, saveKey)
    local r = {}
    for k, v in pairs(t) do
        if saveKey and type(v) == "table" then v.__key = k end
        r[#r + 1] = v
    end
    return r
end
function table.CollapseKeyValue(t)
    local r = {}
    for _, kv in pairs(t) do r[kv.Key] = kv.Value end
    return r
end
function table.Flip(t) local r = {}; for k, v in pairs(t) do r[v] = k end return r end
function table.ForceInsert(t, v) t = t or {}; table.insert(t, v); return t end
function table.LowerKeyNames(t)
    local r = {}
    for k, v in pairs(t) do
        if type(v) == "table" then v = table.LowerKeyNames(v) end
        r[type(k) == "string" and string.lower(k) or k] = v
    end
    return r
end
function table.MemberValuesFromKey(t, key)
    local r = {}
    for _, v in pairs(t) do if type(v) == "table" and v[key] ~= nil then r[#r + 1] = v[key] end end
    return r
end
function table.GetWinningKey(t)
    local bk, bv
    for k, v in pairs(t) do if bv == nil or v > bv then bk, bv = k, v end end
    return bk
end
function table.FindNext(t, v)
    local found = false
    for _, x in pairs(t) do
        if found then return x end
        if x == v then found = true end
    end
    return (next(t) and select(2, next(t))) or nil
end
function table.FindPrev(t, v)
    local prev
    for _, x in pairs(t) do
        if x == v then if prev ~= nil then return prev end return table.GetLastValue(t) end
        prev = x
    end
end
function table.SortByKey(t, desc)
    local keys = table.GetKeys(t)
    if desc then table.sort(keys, function(a, b) return a > b end) else table.sort(keys) end
    return keys
end
function table.SortByMember(t, member, asc)
    table.sort(t, function(a, b)
        local x, y = a[member], b[member]
        if asc then return x < y end
        return x > y
    end)
end
function table.SortDesc(t) table.sort(t, function(a, b) return a > b end) return t end
function table.Sanitise(t, done)
    done = done or {}
    local r = {}
    for k, v in pairs(t) do
        if isvec(v) then r[k] = { __type = "Vector", x = v.x, y = v.y, z = v.z }
        elseif type(v) == "table" and getmetatable(v) == ANG then r[k] = { __type = "Angle", p = v.p, y = v.y, r = v.r }
        elseif type(v) == "table" and not done[v] then done[v] = true; r[k] = table.Sanitise(v, done)
        elseif type(v) == "boolean" then r[k] = { __type = "Bool", tostring(v) }
        else r[k] = v end
    end
    return r
end
function table.DeSanitise(t)
    local r = {}
    for k, v in pairs(t) do
        if type(v) == "table" then
            if v.__type == "Vector" then r[k] = Vector(v.x, v.y, v.z)
            elseif v.__type == "Angle" then r[k] = Angle(v.p, v.y, v.r)
            elseif v.__type == "Bool" then r[k] = v[1] == "true"
            else r[k] = table.DeSanitise(v) end
        else r[k] = v end
    end
    return r
end
function table.ToString(t, name, nice)
    local nl, tab = nice and "\n" or "", nice and "\t" or ""
    local out = {}
    local function dump(v, depth, done)
        if type(v) == "table" then
            if done[v] then return "{ --[[cycle]] }" end
            done[v] = true
            local parts = { "{" .. nl }
            for k, x in pairs(v) do
                local ks = type(k) == "string" and string.format("[%q]", k) or "[" .. tostring(k) .. "]"
                parts[#parts + 1] = string.rep(tab, depth + 1) .. ks .. " = " .. dump(x, depth + 1, done) .. "," .. nl
            end
            parts[#parts + 1] = string.rep(tab, depth) .. "}"
            return table.concat(parts)
        elseif type(v) == "string" then return string.format("%q", v) end
        return tostring(v)
    end
    return (name and (name .. " = ") or "") .. dump(t, 0, {}) .. nl
end

------------------------------------------------------------------------ bit (32-bit, via Luau bit32)
bit = {}
local function tobit(n) n = bit32.band(floor(n), 0xFFFFFFFF); if n >= 0x80000000 then n -= 0x100000000 end return n end
bit.tobit = tobit
function bit.band(...) return tobit(bit32.band(...)) end
function bit.bor(...) return tobit(bit32.bor(...)) end
function bit.bxor(...) return tobit(bit32.bxor(...)) end
function bit.bnot(n) return tobit(bit32.bnot(n)) end
function bit.lshift(n, s) return tobit(bit32.lshift(n, bit32.band(s, 31))) end
function bit.rshift(n, s) return tobit(bit32.rshift(n, bit32.band(s, 31))) end
function bit.arshift(n, s) return tobit(bit32.arshift(n, bit32.band(s, 31))) end
function bit.rol(n, s) return tobit(bit32.lrotate(n, s)) end
function bit.ror(n, s) return tobit(bit32.rrotate(n, s)) end
function bit.bswap(n) return tobit(bit32.byteswap(n)) end
function bit.tohex(n, len)
    len = len or 8
    local upper = len < 0
    len = abs(len)
    local s = string.format("%0" .. len .. "x", bit32.band(n, 0xFFFFFFFF))
    s = string.sub(s, -len)
    return upper and string.upper(s) or s
end

------------------------------------------------------------------------ utf8
function utf8.GetChar(s, i) local o = utf8.offset(s, i); if not o then return nil end local o2 = utf8.offset(s, i + 1); return string.sub(s, o, (o2 or #s + 1) - 1) end
function utf8.sub(s, i, j)
    local n = utf8.len(s) or #s
    i = i or 1; j = j or -1
    if i < 0 then i = n + i + 1 end
    if j < 0 then j = n + j + 1 end
    if i < 1 then i = 1 end
    if j > n then j = n end
    if i > j then return "" end
    local a = utf8.offset(s, i)
    local b = utf8.offset(s, j + 1)
    return string.sub(s, a, (b or #s + 1) - 1)
end
function utf8.force(s)
    local out, i = {}, 1
    while i <= #s do
        local ok, cp = pcall(utf8.codepoint, s, i)
        if ok then
            local ch = utf8.char(cp)
            out[#out + 1] = ch; i += #ch
        else
            out[#out + 1] = "\239\191\189"; i += 1
        end
    end
    return table.concat(out)
end

------------------------------------------------------------------------ hook
hook = {}
local hooks = {}   -- event -> { [id] = fn }
local order = {}   -- event -> { id, ... } in insertion order (a re-added id keeps its place)
function hook.Add(event, id, fn)
    if type(fn) ~= "function" then return end
    if not hooks[event] then hooks[event] = {}; order[event] = {} end
    if hooks[event][id] == nil then table.insert(order[event], id) end
    hooks[event][id] = fn
end
function hook.Remove(event, id)
    local t = hooks[event]
    if not t or t[id] == nil then return end
    t[id] = nil
    for i, v in ipairs(order[event]) do if v == id then table.remove(order[event], i) break end end
end
function hook.GetTable() return hooks end
function hook.Call(event, gm, ...)
    local t = hooks[event]
    if t then
        local ids = order[event]
        local i = 1
        while i <= #ids do
            local id = ids[i]
            local fn = t[id]
            local a, b, c, d, e, f
            if fn == nil then
                table.remove(ids, i)
                continue
            elseif type(id) == "string" then
                a, b, c, d, e, f = fn(...)
            elseif type(id) == "table" and id.IsValid and not id:IsValid() then
                t[id] = nil
                table.remove(ids, i)
                continue
            else
                a, b, c, d, e, f = fn(id, ...)
            end
            if a ~= nil then return a, b, c, d, e, f end
            i += 1
        end
    end
    if gm and type(gm[event]) == "function" then return gm[event](gm, ...) end
end
function hook.Run(event, ...) return hook.Call(event, GM or GAMEMODE, ...) end
-- the own event bus feeds the documented hooks
if events then
    -- the game's order (luabind.md F27/F68, gap-report #206): Think, then the timer pass,
    -- then Tick; `tick.timers` is dispatched right after the timers run
    events.on("tick.pre", function() hook.Run("Think") end)
    events.on("tick.timers", function() hook.Run("Tick") end)
end

------------------------------------------------------------------------ timer / time
-- the host prelude's `timer` (measured against the game: Simple-first-then-name order, due
-- times in fractional ticks, errors destroy only that timer) is kept; the documented extras:
if timer then
    function timer.Check() return true end
end
local function curtime()
    if game and game.tick and game.interval then return game.tick() * game.interval() end
    return 0
end
function CurTime() return curtime() end
function RealTime() return curtime() end
function SysTime() return __glua_systime() end
function FrameTime() return game and game.interval and game.interval() or 0.015 end

__glua_classes = __glua_classes or {}
__glua_classes.Vector = VEC
__glua_classes.Angle = ANG
__glua_classes.Color = COL
__glua_classes.COLOR = COL
