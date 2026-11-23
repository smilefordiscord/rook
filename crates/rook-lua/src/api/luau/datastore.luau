-- Data store requests (api/methods/data_stores/requests.rs): each spends
-- its budget or waits its turn in the queue, then runs. UpdateAsync's
-- transform runs between its read and its write, and may not yield
-- (creator-docs GlobalDataStore:UpdateAsync).
local admit, poll, run, wait, cancel, create, resume, status, error = ...

local ds = {}

local function request(name, store, ...)
	local ticket, delay = admit(name, store, ...)
	while ticket ~= nil and delay ~= nil do
		wait(delay)
		delay = poll(ticket)
	end
end

function ds.method(name)
	return function(self, ...)
		request(name, self, ...)
		return run(name, self, ...)
	end
end

function ds.update(self, key, transform)
	request("UpdateAsync", self, key, transform)
	local old, info = run("UpdateAsync", self, key, transform)
	local co = create(transform)
	local ok, value, userIds, metadata = resume(co, old, info)
	if not ok then
		error(value, 0)
	end
	if status(co) ~= "dead" then
		cancel(co)
		error("UpdateAsync's transform function can't yield", 0)
	end
	-- A nil value cancels the write (creator-docs).
	if value == nil then
		return nil
	end
	return run("UpdateWrite", self, key, value, userIds, metadata)
end

return ds
