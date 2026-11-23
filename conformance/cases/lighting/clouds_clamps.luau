-- Clouds as scripts see them: its defaults, what each number reads back
-- after an out-of-range, huge, infinite or NaN value, and where it may be
-- parented. docs/behavior/lighting/clouds.md. The Clouds is made here and
-- destroyed, and visits Terrain only for a moment.
local function num(x)
	if typeof(x) ~= "number" then
		return typeof(x)
	elseif x ~= x then
		return "nan"
	elseif x == math.huge then
		return "inf"
	elseif x == -math.huge then
		return "-inf"
	end
	local s = ("%.4g"):format(x)
	return s == "-0" and "0" or s
end

local function color(c)
	return ("%s,%s,%s"):format(num(c.R), num(c.G), num(c.B))
end

local function value(v)
	local t = typeof(v)
	if t == "number" then
		return num(v)
	elseif t == "Color3" then
		return color(v)
	elseif t == "Vector3" then
		return ("%s,%s,%s"):format(num(v.X), num(v.Y), num(v.Z))
	end
	return tostring(v)
end

local function defaults(e, props)
	local parts = {}
	for _, name in props do
		local ok, v = pcall(function()
			return e[name]
		end)
		parts[#parts + 1] = name .. "=" .. (ok and value(v) or "error")
	end
	log(e.ClassName, table.concat(parts, " "))
end

-- Default, then what reads back after each probe.
local probes = { -1, 0.5, 2, 70, 1e9, -1e9, 0 / 0, math.huge, -math.huge }
local function ranges(e, props, values)
	for _, name in props do
		local before = e[name]
		local parts = {}
		for _, v in values or probes do
			local ok = pcall(function()
				e[name] = v
			end)
			parts[#parts + 1] = num(v) .. "=" .. (ok and num(e[name]) or "error")
		end
		log(e.ClassName, name, num(before), table.concat(parts, " "))
	end
end

-- Clouds: values, then where it can go.
local clouds = Instance.new("Clouds")
defaults(clouds, { "Cover", "Density", "Color", "Enabled" })
ranges(clouds, { "Cover", "Density" }, { -1, 0.25, 2, 1e9, 0 / 0, math.huge })
clouds.Cover, clouds.Density = 0.5, 0.7
local folder = Instance.new("Folder")
local function parent(label, p)
	local ok, e = pcall(function()
		clouds.Parent = p
	end)
	local where = clouds.Parent and clouds.Parent.ClassName or "nil"
	log("Clouds parent", label, ok and "ok" or (e:gsub("^[^:]*:%d+: ", "")), where)
	clouds.Parent = nil
end
parent("Folder", folder)
parent("Workspace", workspace)
parent("Terrain", workspace.Terrain)
parent("Lighting", game:GetService("Lighting"))
parent("Part", Instance.new("Part"))
log("Clouds in Terrain stays", (function()
	clouds.Parent = workspace.Terrain
	local here = clouds.Parent == workspace.Terrain
	clouds.Parent = nil
	return here
end)())
folder:Destroy()
clouds:Destroy()
