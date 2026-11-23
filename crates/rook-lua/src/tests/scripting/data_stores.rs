//! DataStoreService (docs/behavior/scripting/data-stores.md).

use crate::tests::*;

#[test]
fn reads_return_the_value_and_its_key_info() {
    expect(
        r#"
        local dss = game:GetService("DataStoreService")
        local ds = dss:GetDataStore("Stats")
        print(ds == dss:GetDataStore("Stats"), ds.ClassName, ds == dss:GetDataStore("Stats", "other"))
        local options = Instance.new("DataStoreSetOptions")
        options:SetMetadata({ tag = "a" })
        local version = ds:SetAsync("p1", { cash = 5, items = { "sword" } }, { 101 }, options)
        local v, info = ds:GetAsync("p1")
        v.cash = 99
        print(ds:GetAsync("p1").cash, ds:GetAsync("p1").items[1])
        print(info.ClassName, info.Version == version, info.CreatedTime == info.UpdatedTime)
        print(info:GetUserIds()[1], info:GetMetadata().tag, options:GetMetadata().tag)
        task.wait(1)
        ds:SetAsync("p1", 7)
        local _, later = ds:GetAsync("p1")
        print(later.CreatedTime == info.CreatedTime, later.UpdatedTime > info.UpdatedTime)
        print(#later:GetUserIds(), next(later:GetMetadata()))
        print(ds:GetAsync("none"))
        print(ds:IncrementAsync("visits"), (ds:IncrementAsync("visits", 4)))
        local n, ninfo = ds:IncrementAsync("visits", 2.9)
        print(n, ninfo.ClassName)
        "#,
        120,
        &[
            "true DataStore false",
            "5 sword",
            "DataStoreKeyInfo true true",
            "101 a a",
            "true true",
            "0 nil",
            "nil nil",
            "1 5",
            "7 DataStoreKeyInfo",
        ],
    );
}

#[test]
fn update_async_passes_key_info_and_replaces_ids_and_metadata() {
    expect(
        r#"
        local ds = game:GetService("DataStoreService"):GetDataStore("Stats")
        ds:SetAsync("p1", { cash = 5 }, { 7 })
        local seen
        local value, info = ds:UpdateAsync("p1", function(old, info)
            seen = info:GetUserIds()[1]
            old.cash += 1
            return old, info:GetUserIds(), { saved = true }
        end)
        print(seen, value.cash, info:GetUserIds()[1], info:GetMetadata().saved)
        print(ds:UpdateAsync("p1", function() return nil end))
        print(ds:GetAsync("p1").cash)
        local _, cleared = ds:UpdateAsync("p1", function(old) return old end)
        print(#cleared:GetUserIds(), next(cleared:GetMetadata()))
        print(pcall(function() ds:UpdateAsync("p1", function() task.wait() return 1 end) end))
        print(pcall(function() ds:UpdateAsync("p1", function() error("boom", 0) end) end))
        print(ds:UpdateAsync("new", function(old, info) return old == nil and info == nil end))
        "#,
        10,
        &[
            "7 6 7 true",
            "nil",
            "6",
            "0 nil",
            "false UpdateAsync's transform function can't yield",
            "false boom",
            "true DataStoreKeyInfo",
        ],
    );
}

#[test]
fn removing_leaves_a_deleted_version() {
    expect(
        r#"
        local dss = game:GetService("DataStoreService")
        -- More listing than a server's 5 a minute.
        dss:SetRateLimitForRequestType(Enum.DataStoreRequestType.StandardList, 10000, 0)
        local ds = dss:GetDataStore("Stats")
        local first = ds:SetAsync("k", "one")
        task.wait(1)
        ds:SetAsync("k", "two")
        task.wait(1)
        local old, info = ds:RemoveAsync("k")
        print(old, info.Version ~= first, ds:GetAsync("k"))
        print(ds:RemoveAsync("k"))
        print(ds:GetVersionAsync("k", first))
        local pages = ds:ListVersionsAsync("k")
        print(pages.ClassName, pages.IsFinished)
        for _, v in pages:GetCurrentPage() do
            print(v.ClassName, v.IsDeleted, v.Version == first)
        end
        local newest = ds:ListVersionsAsync("k", Enum.SortDirection.Descending, 0, 0, 1)
        print(newest:GetCurrentPage()[1].IsDeleted, newest.IsFinished)
        newest:AdvanceToNextPageAsync()
        print(newest:GetCurrentPage()[1].IsDeleted, newest.IsFinished)
        local t = ds:ListVersionsAsync("k"):GetCurrentPage()[2].CreatedTime
        print(ds:GetVersionAtTimeAsync("k", t))
        print((ds:GetVersionAtTimeAsync("k", t - 1)))
        print(ds:GetVersionAtTimeAsync("k", t + 1500))
        print((ds:GetVersionAtTimeAsync("k", t - 5000)))
        print(pcall(function() return ds:GetVersionAtTimeAsync("k", 0) end))
        ds:RemoveVersionAsync("k", first)
        print(ds:GetVersionAsync("k", first))
        print(#ds:ListVersionsAsync("k"):GetCurrentPage())
        local recent = ds:ListVersionsAsync("k", Enum.SortDirection.Ascending, t)
        print(#recent:GetCurrentPage())
        ds:SetAsync("k", "back")
        print(ds:GetAsync("k"))
        "#,
        200,
        &[
            "two true nil nil",
            "nil nil",
            "one DataStoreKeyInfo",
            "DataStoreVersionPages true",
            "DataStoreObjectVersionInfo false true",
            "DataStoreObjectVersionInfo false false",
            "DataStoreObjectVersionInfo true false",
            "true false",
            "false false",
            "two DataStoreKeyInfo",
            "one",
            "nil nil",
            "nil",
            "false Timestamp must be positive and not more than ten minutes in the future.",
            "nil nil",
            "2",
            "2",
            "back DataStoreKeyInfo",
        ],
    );
}

#[test]
fn keys_and_stores_list_a_page_at_a_time() {
    expect(
        r#"
        local dss = game:GetService("DataStoreService")
        -- More listing than a server's 5 a minute.
        dss:SetRateLimitForRequestType(Enum.DataStoreRequestType.StandardList, 10000, 0)
        task.wait(1)
        local ds = dss:GetDataStore("Inv")
        for _, k in { "a1", "a2", "a3", "b1" } do ds:SetAsync(k, 1) end
        dss:GetDataStore("Inv", "houses"):SetAsync("h1", 1)
        ds:RemoveAsync("a2")
        local pages = ds:ListKeysAsync("a", 2)
        local function show(p)
            local names = {}
            for _, key in p:GetCurrentPage() do table.insert(names, key.KeyName) end
            print(table.concat(names, ","), p.IsFinished, p.Cursor)
        end
        show(pages)
        pages:AdvanceToNextPageAsync()
        show(pages)
        print(pcall(function() pages:AdvanceToNextPageAsync() end))
        show(ds:ListKeysAsync("", 0, "", true))
        show(ds:ListKeysAsync("", 0, "a2"))
        print(ds:ListKeysAsync():GetCurrentPage()[1].ClassName)
        local options = Instance.new("DataStoreOptions")
        options.AllScopes = true
        local all = dss:GetDataStore("Inv", "", options)
        show(all:ListKeysAsync("", 3))
        print(all:GetAsync("houses/h1"), (ds:GetAsync("h1")))
        print(pcall(function() return all:GetAsync("h1") end))
        print(pcall(function() return dss:GetDataStore("Inv", "global", options) end))
        dss:GetDataStore("Other"):SetAsync("x", 1)
        dss:GetDataStore("Empty")
        local stores = dss:ListDataStoresAsync()
        for _, s in stores:GetCurrentPage() do
            print(s.ClassName, s.DataStoreName, s.CreatedTime <= s.UpdatedTime)
        end
        print(#dss:ListDataStoresAsync("O"):GetCurrentPage(), stores.IsFinished)
        "#,
        120,
        &[
            "a1,a2 false a2",
            "a3 true ",
            "false No pages to advance to.",
            "a1,a3,b1 true ",
            "a3,b1 true ",
            "DataStoreKey",
            "global/a1,global/a2,global/a3 false global/a3",
            "1 nil",
            "false Keys must start with their scope, as in \"scope/key\", when AllScopes is true.",
            "false The scope must be empty when DataStoreOptions.AllScopes is true.",
            "DataStoreInfo Inv true",
            "DataStoreInfo Other true",
            "1 true",
        ],
    );
}

#[test]
fn ordered_stores_sort_into_pages() {
    expect(
        r#"
        local od = game:GetService("DataStoreService"):GetOrderedDataStore("Top")
        for i, n in { 5, 1, 9, 7 } do od:SetAsync("k" .. i, n) end
        print(od:GetAsync("k1"), select('#', od:GetAsync("k1")))
        local pages = od:GetSortedAsync(false, 2)
        print(pages.ClassName, pages:IsA("Pages"), pages.IsFinished)
        local page = pages:GetCurrentPage()
        print(page[1].key, page[1].value, page[2].value, #page)
        pages:AdvanceToNextPageAsync()
        page = pages:GetCurrentPage()
        print(page[1].value, page[2].value, pages.IsFinished)
        local ranged = od:GetSortedAsync(true, 10, 2, 8):GetCurrentPage()
        print(#ranged, ranged[1].value, ranged[2].value)
        print(od:IncrementAsync("k2", 10), od:RemoveAsync("k3"), od:GetAsync("k3"))
        print(od:GetSortedAsync(false, 1):GetCurrentPage()[1].key)
        print(od:BatchGetAsync({ "k1", "k3" }).k1.value, od:BatchGetAsync({ "k3" }).k3)
        print(pcall(function() od:SetAsync("x", 1.5) end))
        print(pcall(function() od:SetAsync("x", "5") end))
        print(pcall(function() od:GetSortedAsync(true, 101) end))
        print(pcall(function() od:GetSortedAsync(true, 10, 1.5) end))
        print(pcall(function() od:GetSortedAsync(true, 10, 5, 1) end))
        print(pcall(function() od:GetSortedAsync(true, 10, nil, 1.5) end))
        local ds = game:GetService("DataStoreService"):GetDataStore("Plain")
        print(pcall(function() ds:BatchGetAsync({ "a" }) end))
        "#,
        10,
        &[
            "5 1",
            "DataStorePages true false",
            "k3 9 7 2",
            "5 1 true",
            "2 5 7",
            "11 9 nil",
            "k2",
            "5 nil",
            "false 103: double is not allowed in data stores.",
            "false 103: string is not allowed in data stores.",
            "false 106: PageSize must be within a predefined range.",
            "false 106: MinValue must be an integer.",
            "false 107: MaxValue must be greater than or equal to MinValue.",
            "false 106: MaxValue must be an integer.",
            "false BatchGetAsync is only supported on OrderedDataStore.",
        ],
    );
}

#[test]
fn values_and_names_within_the_limits() {
    expect(
        r#"
        local dss = game:GetService("DataStoreService")
        local ds = dss:GetDataStore("Limits")
        local function try(f) print(select(2, pcall(f))) end
        try(function() ds:SetAsync("", 1) end)
        try(function() ds:SetAsync(string.rep("k", 51), 1) end)
        print(ds:SetAsync(string.rep("k", 50), 1) ~= nil)
        try(function() ds:SetAsync("k", workspace) end)
        try(function() ds:SetAsync("k", { workspace }) end)
        try(function() ds:SetAsync("k", { a = Vector3.one }) end)
        try(function() ds:SetAsync("k", "\255") end)
        try(function() ds:SetAsync("k", 0 / 0) end)
        local cycle = {} cycle.self = cycle
        try(function() ds:SetAsync("k", cycle) end)
        try(function() ds:SetAsync("k") end)
        try(function() ds:SetAsync("k", string.rep("x", 4194303)) end)
        print(ds:SetAsync("k", string.rep("x", 4194302)) ~= nil)
        ds:SetAsync("k", { 1, 2, x = 3 })
        local mixed = ds:GetAsync("k")
        print(#mixed, mixed.x)
        ds:SetAsync("k", { [2] = "b" })
        print(ds:GetAsync("k")["2"])
        try(function() ds:SetAsync("k", 1, { 1, 2, 3, 4, 5 }) end)
        try(function() ds:SetAsync("k", 1, { "me" }) end)
        local options = Instance.new("DataStoreSetOptions")
        options:SetMetadata({ big = string.rep("x", 300) })
        try(function() ds:SetAsync("k", 1, {}, options) end)
        try(function() ds:UpdateAsync("k", function() return 1, {}, "meta" end) end)
        try(function() dss:GetDataStore("") end)
        try(function() dss:GetDataStore(string.rep("n", 51)) end)
        try(function() dss:GetDataStore("n", "") end)
        try(function() dss:GetOrderedDataStore("n", string.rep("s", 51)) end)
        "#,
        10,
        &[
            "101: Key name can't be empty.",
            "102: Key name exceeds the 50 character limit.",
            "true",
            "104: Cannot store Instance in data store. Data stores can only accept valid UTF-8 characters.",
            "104: Cannot store Array in data store. Data stores can only accept valid UTF-8 characters.",
            "104: Cannot store Dictionary in data store. Data stores can only accept valid UTF-8 characters.",
            "104: Cannot store string in data store. Data stores can only accept valid UTF-8 characters.",
            "104: Cannot store number in data store. Data stores can only accept valid UTF-8 characters.",
            "104: Cannot store Dictionary in data store. Data stores can only accept valid UTF-8 characters.",
            "Argument 2 missing or nil",
            "105: Serialized value exceeds 4MB limit.",
            "true",
            "2 nil",
            "b",
            "512: UserID size exceeds 4 limit.",
            "513: Attribute userId format is invalid.",
            "511: Metadata attribute size exceeds 300 limit.",
            "513: Attribute metadata format is invalid.",
            "DataStore name can't be empty.",
            "DataStore name can't be longer than 50 characters.",
            "DataStore scope can't be empty.",
            "DataStore scope can't be longer than 50 characters.",
        ],
    );
}

#[test]
fn request_budgets_start_full_and_refill() {
    expect(
        r#"
        local dss = game:GetService("DataStoreService")
        local T = Enum.DataStoreRequestType
        local function budgets(...)
            local out = {}
            for _, t in { ... } do table.insert(out, dss:GetRequestBudgetForRequestType(t)) end
            print(table.concat(out, " "))
        end
        budgets(T.StandardRead, T.StandardWrite, T.StandardList, T.StandardRemove, T.OrderedWrite, T.OrderedList)
        budgets(T.GetAsync, T.SetIncrementAsync, T.UpdateAsync, T.GetSortedAsync, T.ListAsync, T.RemoveVersionAsync)
        local ds = dss:GetDataStore("B")
        ds:SetAsync("k", 1)
        ds:GetAsync("k")
        ds:GetAsync("other")
        budgets(T.StandardRead, T.StandardWrite, T.UpdateAsync)
        local fresh = Instance.new("DataStoreGetOptions")
        fresh.UseCache = false
        ds:GetAsync("k", fresh)
        budgets(T.StandardRead)
        ds:UpdateAsync("k", function(v) return v end)
        budgets(T.StandardRead, T.StandardWrite)
        task.wait(5)
        ds:GetAsync("k")
        budgets(T.StandardRead)
        print(pcall(function() dss:SetRateLimitForRequestType(T.UpdateAsync, 1, 1) end))
        print(pcall(function() dss:SetRateLimitForRequestType(T.GetAsync, 61, 0) end))
        dss:SetRateLimitForRequestType(T.StandardList, 0, 0)
        budgets(T.StandardList)
        "#,
        300,
        &[
            "60 60 5 60 30 0",
            "60 60 60 5 5 5",
            "59 59 59",
            "58",
            "57 58",
            "59",
            "false UpdateAsync can't be configured with SetRateLimitForRequestType.",
            "false GetAsync's baseLimit must be from 0 to 60.",
            "0",
        ],
    );
}

#[test]
fn spent_budgets_queue_requests_until_they_refill() {
    let mut lines = vec!["start".to_owned()];
    let warning = "WARN DataStore request was added to queue. If request queue fills, further requests will be dropped. Try sending fewer requests.";
    lines.push(warning.to_owned());
    lines.push("waited 12".to_owned());
    lines.extend((0..30).map(|i| format!("{warning}Key = q{i}")));
    lines.push(
        "false 301: GetAsync request dropped. Request was throttled but queue was full.".to_owned(),
    );
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    expect(
        r#"
        local dss = game:GetService("DataStoreService")
        local ds = dss:GetDataStore("Q")
        print("start")
        for _ = 1, 5 do ds:ListKeysAsync() end
        local before = time()
        ds:ListKeysAsync()
        print("waited", math.round(time() - before))
        local fresh = Instance.new("DataStoreGetOptions")
        fresh.UseCache = false
        for i = 1, 60 do ds:GetAsync("k" .. i, fresh) end
        for i = 0, 29 do task.spawn(ds.GetAsync, ds, "q" .. i) end
        print(pcall(function() ds:GetAsync("dropped") end))
        "#,
        60 * 14,
        &lines,
    );
}

/// An increment past a 64-bit integer raises and leaves the value as it
/// was, in either kind of store, instead of wrapping around (or crashing
/// a build that checks overflow). User ids are whole numbers.
#[test]
fn increments_and_user_ids_stay_whole_numbers() {
    expect(
        r#"
        local dss = game:GetService("DataStoreService")
        local ds = dss:GetDataStore("Big")
        local od = dss:GetOrderedDataStore("Big")
        local function try(f) print(select(2, pcall(f))) end
        ds:IncrementAsync("k", 2^62)
        try(function() ds:IncrementAsync("k", 2^62) end)
        print(ds:GetAsync("k") == 2^62)
        od:SetAsync("k", 2^62)
        try(function() od:IncrementAsync("k", 2^62) end)
        print(od:GetAsync("k") == 2^62)
        try(function() ds:SetAsync("u", 1, { 1.5 }) end)
        try(function() ds:SetAsync("u", 1, { 0/0 }) end)
        "#,
        60,
        &[
            "IncrementAsync's result is outside the range of a 64-bit integer.",
            "true",
            "IncrementAsync's result is outside the range of a 64-bit integer.",
            "true",
            "513: Attribute userId format is invalid.",
            "513: Attribute userId format is invalid.",
        ],
    );
}
