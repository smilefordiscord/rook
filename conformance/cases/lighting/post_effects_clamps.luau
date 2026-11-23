-- The PostEffects as scripts see them: their defaults, and what each
-- number reads back after an out-of-range, huge, infinite or NaN value.
-- docs/behavior/lighting/post-processing.md. Every effect is made here,
-- kept out of Lighting, and destroyed.
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

-- The PostEffects.
local cc = Instance.new("ColorCorrectionEffect")
defaults(cc, { "Enabled", "Brightness", "Contrast", "Saturation", "TintColor" })
ranges(cc, { "Brightness", "Contrast", "Saturation" })
cc.TintColor = Color3.new(2, -1, 0.5)
log("TintColor out of range", color(cc.TintColor))
log("IsA PostEffect", cc:IsA("PostEffect"))
cc:Destroy()

local blur = Instance.new("BlurEffect")
defaults(blur, { "Enabled", "Size" })
ranges(blur, { "Size" }, { -1, 0.5, 56, 100, 1e9, 0 / 0, math.huge })
blur:Destroy()

local bloom = Instance.new("BloomEffect")
defaults(bloom, { "Enabled", "Intensity", "Size", "Threshold" })
ranges(bloom, { "Intensity", "Threshold" })
ranges(bloom, { "Size" }, { -1, 0.5, 56, 100, 1e9, 0 / 0, math.huge })
bloom:Destroy()

local rays = Instance.new("SunRaysEffect")
defaults(rays, { "Enabled", "Intensity", "Spread" })
ranges(rays, { "Intensity", "Spread" })
rays:Destroy()

local dof = Instance.new("DepthOfFieldEffect")
defaults(dof, { "Enabled", "FarIntensity", "FocusDistance", "InFocusRadius", "NearIntensity" })
ranges(dof, { "FarIntensity", "NearIntensity" })
ranges(dof, { "FocusDistance", "InFocusRadius" }, { -1, 0.5, 200, 1000, 1e9, 0 / 0, math.huge })
dof:Destroy()

local grading = Instance.new("ColorGradingEffect")
defaults(grading, { "Enabled", "TonemapperPreset" })
grading:Destroy()
