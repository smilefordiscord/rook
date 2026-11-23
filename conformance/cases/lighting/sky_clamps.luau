-- Sky and Atmosphere as scripts see them: their defaults, and what each
-- number reads back after an out-of-range, huge, infinite or NaN value.
-- docs/behavior/lighting/sky.md, atmosphere.md. Every instance is made
-- here, kept out of Lighting, and destroyed.
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

-- Sky.
local sky = Instance.new("Sky")
defaults(sky, {
	"CelestialBodiesShown",
	"MoonAngularSize",
	"SunAngularSize",
	"StarCount",
	"SkyboxOrientation",
})
defaults(sky, { "SunTextureId", "MoonTextureId" })
defaults(sky, { "SkyboxBk", "SkyboxDn", "SkyboxFt", "SkyboxLf", "SkyboxRt", "SkyboxUp" })
defaults(sky, { "SunTextureContent", "MoonTextureContent", "SkyboxBackContent", "SkyboxUpContent" })
ranges(sky, { "SunAngularSize", "MoonAngularSize" })
ranges(sky, { "StarCount" }, { -1, 0, 2.5, 5000, 1e5, 1e6, 1e9, 0 / 0 })
for _, v in { Vector3.new(90, -45, 10), Vector3.new(400, -720, 1e9), Vector3.new(0 / 0, 0, 0) } do
	local ok = pcall(function()
		sky.SkyboxOrientation = v
	end)
	log("SkyboxOrientation", value(v), ok and value(sky.SkyboxOrientation) or "error")
end
sky.SkyboxBk = "rbxassetid://1"
log("SkyboxBk set", sky.SkyboxBk, sky.SkyboxBackContent)
sky.SunTextureId = ""
log("SunTextureId empty", ("%q"):format(sky.SunTextureId), sky.SunTextureContent)
log("CelestialBodiesShown number", caught(function()
	sky.CelestialBodiesShown = 0
	return sky.CelestialBodiesShown
end))
sky:Destroy()

-- Atmosphere.
local atmosphere = Instance.new("Atmosphere")
defaults(atmosphere, { "Density", "Offset", "Haze", "Glare", "Color", "Decay" })
ranges(atmosphere, { "Density", "Offset", "Haze", "Glare" }, { -1, 0.5, 2, 11, 1e9, 0 / 0, math.huge })
atmosphere.Color = Color3.new(2, -1, 0.5)
log("Atmosphere Color out of range", color(atmosphere.Color))
atmosphere:Destroy()
