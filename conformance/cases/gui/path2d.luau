-- Path2D and Path2DControlPoint as a server script sees them: the
-- datatype (constructor, members, equality, errors), the path's
-- defaults, its point setters and their errors, and the curve queries on
-- fixed points given in offsets (nothing is laid out on a server, so
-- scale-only points are logged apart). docs/behavior/gui/modern.md#path2d.
local function caught(f, ...)
	local r = table.pack(pcall(f, ...))
	if r[1] then
		local s = {}
		for i = 2, r.n do
			s[#s + 1] = tostring(r[i])
		end
		return "ok", table.concat(s, " ")
	end
	local e = r[2]
	return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end

local function num(x)
	if x ~= x then
		return "nan"
	elseif x == math.huge then
		return "inf"
	elseif x == -math.huge then
		return "-inf"
	end
	local s = ("%.3f"):format(x)
	return s == "-0.000" and "0.000" or s
end

local function udim2(u)
	if typeof(u) ~= "UDim2" then
		return typeof(u)
	end
	return ("{%s,%d}{%s,%d}"):format(num(u.X.Scale), u.X.Offset, num(u.Y.Scale), u.Y.Offset)
end

local function vec2(v)
	if typeof(v) ~= "Vector2" then
		return typeof(v)
	end
	return num(v.X) .. "," .. num(v.Y)
end

local function point(p)
	if typeof(p) ~= "Path2DControlPoint" then
		return typeof(p)
	end
	return udim2(p.Position) .. "/" .. udim2(p.LeftTangent) .. "/" .. udim2(p.RightTangent)
end

-- Two places: Studio finds the place a share of the length along a curve
-- to within about a millionth of its parameter, which can move the third
-- place of the tangent there (66.650 where the exact place gives 66.651).
local function vec2_coarse(v)
	if typeof(v) ~= "Vector2" then
		return typeof(v)
	end
	local function two(x)
		local s = ("%.2f"):format(x)
		return s == "-0.00" and "0.00" or s
	end
	return two(v.X) .. "," .. two(v.Y)
end

local function rect(r)
	if typeof(r) ~= "Rect" then
		return typeof(r)
	end
	return ("%s,%s %s,%s"):format(num(r.Min.X), num(r.Min.Y), num(r.Max.X), num(r.Max.Y))
end

-- tostring without an address, which changes from run to run.
local function str(v)
	return (tostring(v):gsub("0x%x+", "<address>"))
end

local function off(x, y)
	return UDim2.fromOffset(x, y)
end

local function cp(x, y, lx, ly, rx, ry)
	return Path2DControlPoint.new(off(x, y), off(lx or 0, ly or 0), off(rx or 0, ry or 0))
end

-- The datatype.
local P = Path2DControlPoint
log("global", typeof(P), type(P))
local keys = {}
for k, v in pairs(P) do
	keys[#keys + 1] = tostring(k) .. ":" .. typeof(v)
end
table.sort(keys)
log("keys", table.concat(keys, " "))
local empty = P.new()
log("new()", typeof(empty), type(empty), point(empty))
log("tostring new()", str(empty))
local full = P.new(UDim2.new(0.5, 10, 0.25, -20), off(-5, 0), UDim2.new(0.1, 3, 0, 4))
log("new(3)", point(full))
log("tostring new(3)", str(full))
log("new(1)", point(P.new(off(7, 8))))
log("new(nil,nil,x)", caught(function()
	return point(P.new(nil, nil, off(1, 2)))
end))
log("member types", typeof(full.Position), typeof(full.LeftTangent), typeof(full.RightTangent))
log("equal", empty == P.new(), full == P.new(UDim2.new(0.5, 10, 0.25, -20), off(-5, 0), UDim2.new(0.1, 3, 0, 4)))
log("not equal", empty == full, select(2, pcall(P.new, nil, off(0, 1))) == empty, empty == nil, empty == 0)
log("raw equal", rawequal(empty, P.new()))
log("unknown member", caught(function()
	return full.Foo
end))
log("lowercase member", caught(function()
	return full.position
end))
log("number key", caught(function()
	return full[1]
end))
log("assign Position", caught(function()
	full.Position = off(1, 1)
end))
log("new number", caught(P.new, 5))
log("new string", caught(P.new, "x"))
log("new UDim", caught(P.new, UDim.new(1, 2)))
log("new Vector2", caught(P.new, Vector2.new(1, 2)))
log("new tangent number", caught(P.new, off(1, 1), 3))
log("new four args", caught(function()
	return point(P.new(off(1, 1), off(2, 2), off(3, 3), off(4, 4)))
end))
log("method call", caught(function()
	return point(P:new(off(1, 1)))
end))
log("table assign", caught(function()
	P.x = 1
end))
log("compare", caught(function()
	return empty < full
end))
log("concat", caught(function()
	return "x" .. empty
end))
log("arith", caught(function()
	return empty + empty
end))

-- The path's own members.
local path = Instance.new("Path2D")
log("defaults", path.Closed, path.Color3, path.Thickness, path.Visible, path.ZIndex, path.Name)
log("IsA", path:IsA("GuiBase"), path:IsA("GuiBase2d"), path:IsA("GuiObject"))
log("no points", #path:GetControlPoints(), path:GetMaxControlPoints())
log("Transparency", caught(function()
	return path.Transparency
end))
log("GetMaxLength", caught(function()
	return path:GetMaxLength()
end))
log("GetSegmentCount", caught(function()
	return path:GetSegmentCount()
end))
log("AbsolutePosition", caught(function()
	return path.AbsolutePosition
end))
for _, v in { -5, 0, 2.5, 150, 1e9, 0 / 0, math.huge } do
	path.Thickness = v
	log("Thickness", num(v), num(path.Thickness))
end
path.Thickness = 1
path.ZIndex = 2.7
log("ZIndex 2.7", path.ZIndex)
path.ZIndex = 1

-- Queries with too few points.
for _, name in { "GetPositionOnCurve", "GetTangentOnCurve", "GetPositionOnCurveArcLength", "GetTangentOnCurveArcLength" } do
	log(name .. " none", caught(function()
		local v = path[name](path, 0.5)
		return typeof(v) == "UDim2" and udim2(v) or vec2(v)
	end))
end
log("GetLength none", caught(function()
	return num(path:GetLength())
end))
log("GetBoundingRect none", caught(function()
	return rect(path:GetBoundingRect())
end))

-- Setters, with no parent.
local changed, fired = {}, 0
path.Changed:Connect(function(name)
	table.insert(changed, name)
end)
path.ControlPointChanged:Connect(function(...)
	fired += 1
	log("ControlPointChanged args", select("#", ...))
end)
log("SetControlPoints", caught(function()
	path:SetControlPoints({ cp(0, 0), cp(100, 0) })
end))
log("after set", #path:GetControlPoints(), point(path:GetControlPoint(1)), point(path:GetControlPoint(2)))
log("unparented insert", caught(function()
	path:InsertControlPoint(2, cp(50, 50))
end))
log("after insert", #path:GetControlPoints())
log("unparented update", caught(function()
	path:UpdateControlPoint(1, cp(1, 1))
end))
log("unparented remove", caught(function()
	path:RemoveControlPoint(#path:GetControlPoints())
end))
log("after update and remove", #path:GetControlPoints(), point(path:GetControlPoint(1)))
task.wait()
log("ControlPointChanged count", fired)
log("Changed", table.concat(changed, ","))
log("GetControlPoints copies", path:GetControlPoints() ~= path:GetControlPoints(), path:GetControlPoint(1) == path:GetControlPoints()[1])
path:SetControlPoints({})
log("set empty", #path:GetControlPoints())

-- Under a frame of fixed size, inside a ScreenGui that isn't shown.
local gui = Instance.new("ScreenGui")
local frame = Instance.new("Frame")
frame.Size = off(400, 300)
frame.Position = off(50, 20)
frame.Parent = gui
path.Parent = frame
log("frame laid out", vec2(frame.AbsoluteSize), vec2(frame.AbsolutePosition))

path:SetControlPoints({ cp(0, 0), cp(100, 0) })
log("insert parented", caught(function()
	path:InsertControlPoint(2, cp(50, 0))
end))
log("points", #path:GetControlPoints(), point(path:GetControlPoint(2)))

-- Index errors.
local function try(name, ...)
	local args = table.pack(...)
	log(name, caught(function()
		local r = path[name:match("^%a+")](path, table.unpack(args, 1, args.n))
		return typeof(r) == "Path2DControlPoint" and point(r) or tostring(r)
	end))
end
try("GetControlPoint 0", 0)
try("GetControlPoint 4", 4)
try("GetControlPoint -1", -1)
try("GetControlPoint 1.5", 1.5)
try("GetControlPoint 3.9", 3.9)
try("GetControlPoint string", "2")
try("GetControlPoint bad string", "a")
try("GetControlPoint nil", nil)
try("GetControlPoint nan", 0 / 0)
try("GetControlPoint huge", math.huge)
try("RemoveControlPoint 4", 4)
try("RemoveControlPoint 0", 0)
try("UpdateControlPoint 4", 4, cp(1, 1))
try("UpdateControlPoint 0", 0, cp(1, 1))
try("UpdateControlPoint nil point", 1, nil)
try("UpdateControlPoint UDim2", 1, off(1, 1))
try("InsertControlPoint 0", 0, cp(1, 1))
try("InsertControlPoint 5", 5, cp(1, 1))
try("InsertControlPoint string point", 1, "x")
try("InsertControlPoint no point", 1)
try("SetControlPoints nil", nil)
try("SetControlPoints string", "x")
try("SetControlPoints numbers", { 1, 2 })
try("SetControlPoints UDim2s", { off(1, 1) })
try("SetControlPoints mixed", { cp(1, 1), 5 })
log("after bad sets", #path:GetControlPoints())
try("SetControlPoints dictionary", { a = cp(1, 1) })
log("after dictionary", #path:GetControlPoints())
try("SetControlPoints hole", { cp(1, 1), nil, cp(3, 3) })
log("after hole", #path:GetControlPoints())
local many = {}
for i = 1, 101 do
	many[i] = cp(i, 0)
end
try("SetControlPoints 101", many)
log("after 101", #path:GetControlPoints(), point(path:GetControlPoint(#path:GetControlPoints())))
many[101] = nil
try("SetControlPoints 100", many)
try("InsertControlPoint full", 1, cp(0, 0))
log("after full insert", #path:GetControlPoints())

-- Curve queries.
-- A query's result formatted, or its error: one failing query doesn't
-- end the case.
local function q(format, name, ...)
	local args = table.pack(...)
	local ok, v = pcall(function()
		return path[name](path, table.unpack(args, 1, args.n))
	end)
	if ok then
		return format(v)
	end
	return "error(" .. (type(v) == "string" and (v:gsub("^[^:]*:%d+: ", "")) or tostring(v)) .. ")"
end

local function queries(label, ts)
	log(label .. " length", q(num, "GetLength"), "rect", q(rect, "GetBoundingRect"))
	for _, t in ts do
		log(
			label .. " t=" .. num(t),
			q(udim2, "GetPositionOnCurve", t),
			q(vec2, "GetTangentOnCurve", t),
			q(udim2, "GetPositionOnCurveArcLength", t),
			q(vec2_coarse, "GetTangentOnCurveArcLength", t)
		)
	end
end

path:SetControlPoints({ cp(0, 0), cp(100, 0) })
queries("line", { 0, 0.25, 0.5, 1 })
log("line t edges", caught(function()
	return udim2(path:GetPositionOnCurve(-0.5)), udim2(path:GetPositionOnCurve(1.5)), vec2(path:GetTangentOnCurve(2))
end))
log("line t nan", caught(function()
	return udim2(path:GetPositionOnCurve(0 / 0)), vec2(path:GetTangentOnCurve(0 / 0))
end))
log("line t huge", caught(function()
	return udim2(path:GetPositionOnCurve(math.huge)), udim2(path:GetPositionOnCurveArcLength(-math.huge))
end))
log("line t string", caught(function()
	return udim2(path:GetPositionOnCurve("0.5"))
end))
log("line t nil", caught(function()
	return udim2(path:GetPositionOnCurve(nil))
end))
log("line t none", caught(function()
	return udim2(path:GetPositionOnCurve())
end))

path:SetControlPoints({ cp(0, 0, 0, 0, 0, 0), cp(0, 0, 0, 0, 0, 0) })
queries("same place", { 0, 0.5 })

path:SetControlPoints({ cp(0, 0, 0, 0, 50, 0), cp(100, 100, 0, -50, 0, 0) })
queries("bend", { 0, 0.25, 0.5, 0.75, 1 })

path:SetControlPoints({ cp(0, 0), cp(100, 0), cp(100, 300) })
queries("two segments", { 0.25, 0.5, 0.75 })
path.Closed = true
queries("closed", { 0.5, 2 / 3, 0.9 })
path.Closed = false

path:SetControlPoints({ cp(0, 0, 0, 0, 30, 40), cp(60, 0, -30, 40, 0, 0) })
queries("arch", { 0.5 })
for _, v in { 0, 10 } do
	path.Thickness = v
	log("rect thickness", v, q(rect, "GetBoundingRect"))
end
path.Thickness = 1

-- Tangents at an end whose tangent is zero.
path:SetControlPoints({ cp(0, 0), cp(100, 0, -20, 0, 0, 0), cp(100, 100) })
queries("zero ends", { 0, 1 })

-- Scale points: what scale means where nothing is laid out.
path:SetControlPoints({
	Path2DControlPoint.new(UDim2.fromScale(0, 0)),
	Path2DControlPoint.new(UDim2.new(1, 0, 0.5, 10)),
})
queries("scale", { 0.5 })

-- Odd places.
path:SetControlPoints({ cp(0, 0), Path2DControlPoint.new(UDim2.new(0 / 0, 0, 0, 0)) })
log("nan point", caught(function()
	return num(path:GetLength()), udim2(path:GetPositionOnCurve(0.5)), udim2(path:GetPositionOnCurveArcLength(0.5))
end))
path:SetControlPoints({ cp(0, 0), Path2DControlPoint.new(UDim2.new(math.huge, 0, 0, 0)) })
log("huge point", caught(function()
	return num(path:GetLength()), rect(path:GetBoundingRect())
end))
path:SetControlPoints({ cp(0, 0), cp(1e9, 0) })
log("far offset", caught(function()
	return point(path:GetControlPoint(2)), num(path:GetLength())
end))

-- Clone keeps the points.
path:SetControlPoints({ cp(1, 2, 3, 4, 5, 6), cp(7, 8) })
path.Closed = true
path.Thickness = 4
local copy = path:Clone()
log("clone", #copy:GetControlPoints(), point(copy:GetControlPoint(1)), copy.Closed, copy.Thickness, copy.Parent)
copy:SetControlPoints({})
log("clone apart", #copy:GetControlPoints(), #path:GetControlPoints())
copy:Destroy()

gui:Destroy()
log("destroyed", #path:GetControlPoints(), path.Parent)
