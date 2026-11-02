//! The GLua core surface (Vector / Angle / Color, math / string / table / bit / utf8, hook,
//! timer) behaves as documented.
use gm_script::{api::Payload, ActorConfig, ActorSpec, Runtime, Snapshot};
use std::sync::Arc;

fn runtime(source: &str) -> Runtime {
    let mut rt = Runtime::new();
    rt.load(ActorSpec {
        addon: "glua".into(),
        name: "core".into(),
        priority: 0,
        source: source.into(),
        config: ActorConfig::default(),
    })
    .unwrap();
    rt
}

fn eval<T: mlua::FromLuaMulti>(code: &str) -> T {
    let rt = runtime("");
    rt.actors()[0].eval(code).unwrap()
}

#[test]
fn vectors_and_angles() {
    let (len, dot, cross, dist): (f64, f64, String, f64) = eval(
        "local a, b = Vector(3, 4, 0), Vector(1, 0, 0)
         return a:Length(), a:Dot(b), tostring(a:Cross(b)), a:Distance(b)",
    );
    assert_eq!(len, 5.0);
    assert_eq!(dot, 3.0);
    assert_eq!(cross, "0.000000 0.000000 -4.000000");
    assert!((dist - 20f64.sqrt()).abs() < 1e-6);
    let (ops, norm, eq): (String, String, bool) = eval(
        "local a = Vector(1, 2, 3)
         local v = (a + Vector(1, 1, 1)) * 2 - Vector(0, 0, 10) / 2
         local n = Vector(0, 3, 4):GetNormalized()
         return tostring(v), tostring(n), Vector(1, 2, 3) == Vector(1, 2, 3)",
    );
    assert_eq!(ops, "4.000000 6.000000 3.000000");
    assert_eq!(norm, "0.000000 0.600000 0.800000");
    assert!(eq);
    // Source's VectorAngles: +x forward is 0 0 0, straight up is pitch 270 (-90)
    let (fwd, up, back): (String, String, String) = eval(
        "return tostring(Vector(1, 0, 0):Angle()), tostring(Vector(0, 0, 1):Angle()), tostring(Vector(-1, 0, 0):Angle())",
    );
    assert_eq!(fwd, "0.000000 0.000000 0.000000");
    assert_eq!(up, "270.000000 0.000000 0.000000");
    assert_eq!(back, "0.000000 180.000000 0.000000");
    let (f, r, u): (String, String, String) = eval(
        "local a = Angle(0, 90, 0)
         local f, r, u = a:Forward(), a:Right(), a:Up()
         local function s(v) return string.format('%.3f %.3f %.3f', math.Round(v.x, 3), math.Round(v.y, 3), math.Round(v.z, 3)) end
         return s(f), s(r), s(u)",
    );
    assert_eq!(f, "0.000 1.000 0.000");
    assert_eq!(r, "1.000 0.000 0.000");
    assert_eq!(u, "0.000 0.000 1.000");
    let (n, rot): (String, String) = eval(
        "local a = Angle(370, -190, 0) a:Normalize()
         local b = Angle(0, 0, 0) b:RotateAroundAxis(Vector(0, 0, 1), 90)
         return tostring(a), string.format('%.3f %.3f %.3f', b.p, b.y, b.r)",
    );
    assert_eq!(n, "10.000000 170.000000 0.000000");
    assert_eq!(rot, "0.000 90.000 0.000");
    let within: bool = eval("return Vector(1, 1, 1):WithinAABox(Vector(2, 2, 2), Vector(0, 0, 0))");
    assert!(within);
}

#[test]
fn colors() {
    let (hex, hsv, lerp, hue): (String, String, String, f64) = eval(
        "local c = Color(255, 128, 0)
         local h, s, v = c:ToHSV()
         local l = Color(0, 0, 0):Lerp(Color(255, 255, 255, 255), 0.5)
         local d = Color(0, 255, 0) d:SetHue(240)
         return c:ToHex(), string.format('%.1f %.3f %.3f', h, s, v), tostring(l), (d:GetHue())",
    );
    assert_eq!(hex, "#FF8000");
    assert_eq!(hsv, "30.1 1.000 1.000");
    assert_eq!(lerp, "127.5 127.5 127.5 255");
    assert_eq!(hue, 240.0);
    let (hsl, from_hsv): (String, String) =
        eval("return tostring(HSLToColor(120, 1, 0.5)), tostring(HSVToColor(0, 1, 1))");
    assert_eq!(hsl, "0 255 0 255");
    assert_eq!(from_hsv, "255 0 0 255");
}

#[test]
fn math_string_table_bit_utf8() {
    let (round, approach, remap, diff, bin): (f64, f64, f64, f64, String) = eval(
        "return math.Round(2.567, 2), math.Approach(0, 10, 3), math.Remap(5, 0, 10, 0, 100), math.AngleDifference(10, 350), math.IntToBin(10)",
    );
    assert_eq!(round, 2.57);
    assert_eq!(approach, 3.0);
    assert_eq!(remap, 50.0);
    assert_eq!(diff, 20.0);
    assert_eq!(bin, "1010");
    let (explode, replace, trim, comma, nice, ext, file, path): (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    ) = eval(
        "return table.concat(string.Explode(',', 'a,b,,c'), '|'), string.Replace('a.b.c', '.', '%'),
                string.Trim('  x  '), string.Comma(1234567), string.NiceSize(1500),
                string.GetExtensionFromFilename('maps/gm_construct.bsp'), string.GetFileFromFilename('maps/gm_construct.bsp'),
                string.GetPathFromFilename('maps/gm_construct.bsp')",
    );
    assert_eq!(explode, "a|b||c");
    assert_eq!(replace, "a%b%c");
    assert_eq!(trim, "x");
    assert_eq!(comma, "1,234,567");
    assert_eq!(nice, "1.5 KB");
    assert_eq!(ext, "bsp");
    assert_eq!(file, "gm_construct.bsp");
    assert_eq!(path, "maps/");
    let (count, has, copy_independent, keys, rev, seq): (i64, bool, bool, i64, String, bool) = eval(
        "local t = { a = 1, b = 2, 3 }
         local c = table.Copy(t) c.a = 9
         local r = table.Reverse({ 1, 2, 3 })
         return table.Count(t), table.HasValue(t, 2), t.a == 1, #table.GetKeys(t), table.concat(r, ''), table.IsSequential({ 1, 2, 3 })",
    );
    assert_eq!(count, 3);
    assert!(has);
    assert!(copy_independent);
    assert_eq!(keys, 3);
    assert_eq!(rev, "321");
    assert!(seq);
    let (band, bnot, lshift, hex, tobit): (i64, i64, i64, String, i64) = eval(
        "return bit.band(0xFF0F, 0x0FF0), bit.bnot(0), bit.lshift(1, 31), bit.tohex(255), bit.tobit(0xFFFFFFFF)",
    );
    assert_eq!(band, 0x0F00);
    assert_eq!(bnot, -1);
    assert_eq!(lshift, -2147483648);
    assert_eq!(hex, "000000ff");
    assert_eq!(tobit, -1);
    let (sub, ch, len): (String, String, i64) =
        eval("local s = 'héllo' return utf8.sub(s, 2, 3), utf8.GetChar(s, 2), utf8.len(s)");
    assert_eq!(sub, "él");
    assert_eq!(ch, "é");
    assert_eq!(len, 5);
}

#[test]
fn hooks_run_in_order_and_stop_on_a_return() {
    let (sum, ret, after): (i64, String, bool) = eval(
        "local calls = 0
         hook.Add('Test', 'a', function(x) calls += x end)
         hook.Add('Test', 'b', function(x) calls += x * 10 end)
         hook.Run('Test', 1)
         hook.Add('Stop', 'first', function() return 'stopped' end)
         hook.Add('Stop', 'second', function() calls = -1000 end)
         local r = hook.Run('Stop')
         hook.Remove('Test', 'b')
         hook.Run('Test', 1)
         return calls, r, hook.GetTable().Test.b == nil",
    );
    assert_eq!(sum, 12);
    assert_eq!(ret, "stopped");
    assert!(after);
}

#[test]
fn think_and_timers_follow_the_tick() {
    let mut rt = runtime(
        "state.thinks = 0
         state.fired = {}
         hook.Add('Think', 'count', function() state.thinks += 1 end)
         timer.Simple(0.03, function() state.fired[#state.fired + 1] = 'simple' end)
         timer.Create('rep', 0.015, 2, function() state.fired[#state.fired + 1] = 'rep' end)
         timer.Create('paused', 0.015, 0, function() state.fired[#state.fired + 1] = 'never' end)
         timer.Pause('paused')",
    );
    for tick in 1..=4u64 {
        let snap = Snapshot {
            tick,
            ..Default::default()
        };
        let a = &mut rt.actors_mut()[0];
        a.dispatch(&Arc::new(snap), "tick.pre", &Payload::default(), tick)
            .unwrap();
    }
    let (thinks, fired, exists, paused): (i64, String, bool, bool) = rt.actors()[0]
        .eval("return state.thinks, table.concat(state.fired, ','), timer.Exists('rep'), timer.IsPaused('paused')")
        .unwrap();
    assert_eq!(thinks, 4);
    assert_eq!(fired, "rep,simple,rep");
    assert!(!exists, "a 2-rep timer stays after its reps");
    assert!(paused);
}
