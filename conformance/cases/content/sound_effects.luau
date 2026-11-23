-- What scripts see of the sound effects Rook now plays but nobody
-- measured: each legacy SoundEffect's properties set past their range,
-- to NaN and to infinity (do they read back as set?), Priority and
-- Enabled, CompressorSoundEffect.SideChain, and the Audio* effects the
-- conformance case content/audio_api leaves out (AudioFilter,
-- AudioFlanger, AudioTremolo, AudioLimiter, AudioGate, and AudioReverb's
-- and AudioEcho's other properties), with NaN on every Audio* effect.
-- docs/behavior/audio/soundgroups-and-effects.md, audio-api.md. Nothing
-- here is parented or played, so nothing is heard.
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
		return tostring(x)
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

-- Default, then what reads back after -1e9, 1e9, NaN, -inf and inf.
local probes = { -1e9, 1e9, 0 / 0, -math.huge, math.huge }
local function ranges(class, props)
	local e = Instance.new(class)
	for _, name in props do
		local parts = { num(e[name]) }
		for _, v in probes do
			local ok = pcall(function()
				e[name] = v
			end)
			parts[#parts + 1] = ok and num(e[name]) or "error"
		end
		log(class, name, table.concat(parts, " "))
	end
	e:Destroy()
end

-- The legacy effects.
ranges("ChorusSoundEffect", { "Depth", "Mix", "Rate" })
ranges("CompressorSoundEffect", { "Attack", "GainMakeup", "Ratio", "Release", "Threshold" })
ranges("DistortionSoundEffect", { "Level" })
ranges("EchoSoundEffect", { "Delay", "DryLevel", "Feedback", "WetLevel" })
ranges("FlangeSoundEffect", { "Depth", "Mix", "Rate" })
ranges("ReverbSoundEffect", { "DecayTime", "Density", "Diffusion", "DryLevel", "WetLevel" })
ranges("TremoloSoundEffect", { "Depth", "Duty", "Frequency" })
ranges("PitchShiftSoundEffect", { "Octave" })
ranges("EqualizerSoundEffect", { "LowGain", "MidGain", "HighGain" })

-- Small values just outside the creator-docs ranges.
local function near(class, name, values)
	local e = Instance.new(class)
	local parts = {}
	for _, v in values do
		local ok = pcall(function()
			e[name] = v
		end)
		parts[#parts + 1] = num(v) .. "=" .. (ok and num(e[name]) or "error")
	end
	log("near", class, name, table.concat(parts, " "))
	e:Destroy()
end
near("FlangeSoundEffect", "Depth", { 0, 0.005, 1.5 })
near("TremoloSoundEffect", "Frequency", { 0, 0.05, 25 })
near("EchoSoundEffect", "Delay", { 0, 0.005, 6 })
near("CompressorSoundEffect", "Attack", { 0, 0.05, 2 })
near("CompressorSoundEffect", "Ratio", { 0, 0.5, 100 })
near("ChorusSoundEffect", "Rate", { -1, 21 })

-- Priority and Enabled.
local chorus = Instance.new("ChorusSoundEffect")
-- Past int32 and NaN: conformance case content/int_properties.
for _, v in { -5, 2.7, -2.7 } do
	local ok = pcall(function()
		chorus.Priority = v
	end)
	log("Priority", num(v), ok, chorus.Priority)
end
log("Priority string", caught(function()
	chorus.Priority = "3"
	return chorus.Priority
end))
log("Priority bad string", caught(function()
	chorus.Priority = "x"
end))
log("Enabled number", caught(function()
	chorus.Enabled = 0
	return chorus.Enabled
end))
log("Enabled nil", caught(function()
	chorus.Enabled = nil
	return chorus.Enabled
end))
log("IsA", chorus:IsA("SoundEffect"), chorus.ClassName)
chorus:Destroy()

-- CompressorSoundEffect.SideChain takes a Sound or a SoundGroup?
local compressor = Instance.new("CompressorSoundEffect")
log("SideChain", compressor.SideChain)
local sound, group = Instance.new("Sound"), Instance.new("SoundGroup")
log("SideChain Sound", caught(function()
	compressor.SideChain = sound
	return compressor.SideChain == sound
end))
log("SideChain SoundGroup", caught(function()
	compressor.SideChain = group
	return compressor.SideChain == group
end))
log("SideChain Part", caught(function()
	compressor.SideChain = Instance.new("Part")
end))
-- A string: conformance case content/int_properties.
log("SideChain nil", caught(function()
	compressor.SideChain = nil
	return compressor.SideChain
end))
compressor.SideChain = sound
sound:Destroy()
log("SideChain destroyed", compressor.SideChain ~= nil)
compressor:Destroy()
group:Destroy()

-- The Audio* effects content/audio_api leaves out.
ranges("AudioFilter", { "Frequency", "Gain", "Q" })
ranges("AudioFlanger", { "Depth", "Mix", "Rate" })
ranges("AudioTremolo", { "Depth", "Duty", "Frequency", "Shape", "Skew", "Square" })
ranges("AudioLimiter", { "MaxLevel", "Release" })
ranges("AudioGate", { "Attack", "Release" })
ranges("AudioReverb", { "DecayRatio", "EarlyDelayTime", "LateDelayTime", "LowShelfFrequency", "LowShelfGain", "ReferenceFrequency" })
ranges("AudioEcho", { "RampTime" })

-- NaN on the effects content/audio_api covers (it sets only -1e9 and 1e9).
local function nan(class, props)
	local e = Instance.new(class)
	local parts = {}
	for _, name in props do
		local ok = pcall(function()
			e[name] = 0 / 0
		end)
		parts[#parts + 1] = name .. "=" .. (ok and num(e[name]) or "error")
	end
	log("nan", class, table.concat(parts, " "))
	e:Destroy()
end
nan("AudioFader", { "Volume" })
nan("AudioEqualizer", { "LowGain", "MidGain", "HighGain" })
nan("AudioDistortion", { "Level" })
nan("AudioPitchShifter", { "Pitch" })
nan("AudioReverb", { "DecayTime", "Density", "Diffusion", "DryLevel", "WetLevel", "HighCutFrequency" })
nan("AudioChorus", { "Depth", "Mix", "Rate" })
nan("AudioEcho", { "DelayTime", "DryLevel", "Feedback", "WetLevel" })
nan("AudioCompressor", { "Attack", "MakeupGain", "Ratio", "Release", "Threshold" })

-- AudioGate's Threshold is a NumberRange.
local gate = Instance.new("AudioGate")
local function range(r)
	return typeof(r) == "NumberRange" and (num(r.Min) .. ".." .. num(r.Max)) or typeof(r)
end
log("AudioGate Threshold", range(gate.Threshold), gate.Bypass)
for _, r in { NumberRange.new(-200, 100), NumberRange.new(-10, -10), NumberRange.new(-90, -85), NumberRange.new(20, 40) } do
	local ok = pcall(function()
		gate.Threshold = r
	end)
	log("AudioGate Threshold set", range(r), ok, range(gate.Threshold))
end
log("AudioGate Threshold number", caught(function()
	gate.Threshold = 5
end))
log("AudioGate pins", table.concat(gate:GetInputPins(), ","), table.concat(gate:GetOutputPins(), ","))
gate:Destroy()

-- AudioFilter's type and GetGainAt (dB at a frequency).
local filter = Instance.new("AudioFilter")
local names = {}
for _, item in Enum.AudioFilterType:GetEnumItems() do
	names[#names + 1] = item.Name .. "=" .. item.Value
end
log("AudioFilterType", table.concat(names, " "))
log("FilterType", filter.FilterType)
local function gains(label)
	local parts = {}
	for _, hz in { 20, 200, 1000, 2000, 4000, 20000 } do
		local ok, g = pcall(function()
			return filter:GetGainAt(hz)
		end)
		local s = ok and ("%.2f"):format(g) or "error"
		parts[#parts + 1] = hz .. "=" .. (s == "-0.00" and "0.00" or s)
	end
	log("GetGainAt", label, table.concat(parts, " "))
end
gains("default")
for _, item in Enum.AudioFilterType:GetEnumItems() do
	filter.FilterType = item
	filter.Gain = 6
	gains(item.Name)
end
filter.FilterType = Enum.AudioFilterType.Peak
log("GetGainAt bad", caught(function()
	return filter:GetGainAt("x")
end))
log("GetGainAt nil", caught(function()
	return filter:GetGainAt()
end))
log("GetGainAt 0 and negative", caught(function()
	return (("%.2f %.2f"):format(filter:GetGainAt(0), filter:GetGainAt(-100)):gsub("%-0%.00", "0.00"))
end))
log("AudioFilter pins", table.concat(filter:GetInputPins(), ","), table.concat(filter:GetOutputPins(), ","))
filter:Destroy()

for _, class in { "AudioFlanger", "AudioTremolo", "AudioLimiter" } do
	local e = Instance.new(class)
	log("pins", class, e.Bypass, table.concat(e:GetInputPins(), ","), table.concat(e:GetOutputPins(), ","))
	e:Destroy()
end
