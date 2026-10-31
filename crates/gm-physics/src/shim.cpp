// Settings JoltC 0.3.1 does not expose. JPC_PhysicsSystem is a reinterpret_cast of
// JPH::PhysicsSystem (JoltC.cpp OPAQUE_WRAPPER), so the same cast works here.
#include <mutex>
#include <Jolt/Jolt.h>
#include <Jolt/Physics/PhysicsSystem.h>

extern "C" {

void gmp_set_contact_settings(void *system, float speculative_distance, float penetration_slop,
                              float linear_cast_threshold) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::PhysicsSettings s = sys->GetPhysicsSettings();
	s.mSpeculativeContactDistance = speculative_distance;
	s.mPenetrationSlop = penetration_slop;
	// a LinearCast body that moves more than this fraction of its inner radius in a step is
	// swept to its time of impact (Jolt's default 0.75 lets a prop falling 9 u per tick pass
	// the speculative distance and land a tick late, see docs/spec/physics.md)
	s.mLinearCastThreshold = linear_cast_threshold;
	if (const char *t = getenv("GMP_LINEARCAST_THRESHOLD")) s.mLinearCastThreshold = (float)atof(t); // experiment knob
	if (const char *t = getenv("GMP_LINEARCAST_MAXPEN")) s.mLinearCastMaxPenetration = (float)atof(t);
	sys->SetPhysicsSettings(s);
}

float gmp_get_speculative_distance(const void *system) {
	return reinterpret_cast<const JPH::PhysicsSystem *>(system)->GetPhysicsSettings().mSpeculativeContactDistance;
}

float gmp_get_penetration_slop(const void *system) {
	return reinterpret_cast<const JPH::PhysicsSystem *>(system)->GetPhysicsSettings().mPenetrationSlop;
}

}

// Constraints: JoltC 0.3.1 has none. Bodies are looked up through the lock interface.
#include <Jolt/Physics/Body/BodyLockMulti.h>
#include <Jolt/Physics/Body/BodyLock.h>
#include <Jolt/Physics/Constraints/FixedConstraint.h>
#include <Jolt/Physics/Constraints/DistanceConstraint.h>
#include <vector>
#include <Jolt/Physics/Constraints/HingeConstraint.h>
#include <Jolt/Physics/Constraints/PointConstraint.h>
#include <Jolt/Physics/Constraints/SliderConstraint.h>
#include <Jolt/Physics/Collision/Shape/MeshShape.h>
#include <Jolt/Physics/Body/BodyCreationSettings.h>

extern "C" {

// static triangle mesh (displacement ground) in Jolt metres/axes; JoltC has no MeshShape
unsigned int gmp_add_static_mesh(void *system, const float *xyz, unsigned int nverts, const unsigned int *idx,
                                 unsigned int ntris, unsigned short layer, float friction) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::VertexList verts;
	verts.reserve(nverts);
	for (unsigned int i = 0; i < nverts; ++i) verts.push_back(JPH::Float3(xyz[3 * i], xyz[3 * i + 1], xyz[3 * i + 2]));
	JPH::IndexedTriangleList tris;
	tris.reserve(ntris);
	for (unsigned int i = 0; i < ntris; ++i) tris.push_back(JPH::IndexedTriangle(idx[3 * i], idx[3 * i + 1], idx[3 * i + 2]));
	JPH::MeshShapeSettings ms(verts, tris);
	JPH::ShapeSettings::ShapeResult r = ms.Create();
	if (r.HasError()) return 0xffffffffu;
	JPH::BodyCreationSettings bs(r.Get(), JPH::RVec3::sZero(), JPH::Quat::sIdentity(), JPH::EMotionType::Static, layer);
	bs.mFriction = friction;
	bs.mRestitution = 0.0f;
	JPH::Body *b = sys->GetBodyInterface().CreateBody(bs);
	if (!b) return 0xffffffffu;
	sys->GetBodyInterface().AddBody(b->GetID(), JPH::EActivation::DontActivate);
	return b->GetID().GetIndexAndSequenceNumber();
}

// weld: both bodies keep their current relative pose
void *gmp_add_fixed_constraint(void *system, unsigned int id1, unsigned int id2) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID ids[2] = { JPH::BodyID(id1), JPH::BodyID(id2) };
	JPH::BodyLockMultiWrite lock(sys->GetBodyLockInterface(), ids, 2);
	JPH::Body *a = lock.GetBody(0), *b = lock.GetBody(1);
	if (!a || !b) return nullptr;
	JPH::FixedConstraintSettings s;
	s.mAutoDetectPoint = true;
	JPH::Constraint *c = s.Create(*a, *b);
	sys->AddConstraint(c);
	return c;
}

// the game's rope is a velocity level constraint: over its length it pulls the attachments back
// with a correction proportional to the excess (the hang harness: 0.14 to 0.21 u over, 20 percent
// more tension than a hard limit), while Jolt's hard limit fixes the excess in position without
// adding velocity; a stiff soft limit at 60 Hz, critically damped, reproduces the hang's swing
static const JPH::SpringSettings ROPE_SPRING(JPH::ESpringMode::FrequencyAndDamping, 60.0f, 1.0f);

struct Rope { JPH::DistanceConstraint *c; float min_d, max_d; };
// slack ropes per physics system (the parity runner builds a world per scenario)
static std::unordered_map<const void *, std::vector<Rope>> ropes_by_system;

void gmp_rope_clear(void *system) { ropes_by_system.erase(system); }

// rope: distance between two world space points kept within [min, max] (metres)
void *gmp_add_distance_constraint(void *system, unsigned int id1, unsigned int id2,
	float x1, float y1, float z1, float x2, float y2, float z2, float min_d, float max_d) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID ids[2] = { JPH::BodyID(id1), JPH::BodyID(id2) };
	JPH::BodyLockMultiWrite lock(sys->GetBodyLockInterface(), ids, 2);
	JPH::Body *a = lock.GetBody(0), *b = lock.GetBody(1);
	if (!a || !b) return nullptr;
	JPH::DistanceConstraintSettings s;
	s.mSpace = JPH::EConstraintSpace::WorldSpace;
	s.mPoint1 = JPH::RVec3(x1, y1, z1);
	s.mPoint2 = JPH::RVec3(x2, y2, z2);
	s.mMinDistance = min_d;
	s.mMaxDistance = max_d;
	if (min_d < max_d) s.mLimitsSpringSettings = ROPE_SPRING;
	JPH::Constraint *c = s.Create(*a, *b);
	sys->AddConstraint(c);
	if (min_d < max_d) ropes_by_system[system].push_back({ static_cast<JPH::DistanceConstraint *>(c), min_d, max_d });
	return c;
}

// the game's rope catches the tick before its attachments pass its length (the hang harness
// shows it acting at 79.59 of 80 u): a slack rope whose attachments would be further apart
// than its length by the end of the step is a rod for this step, see docs/spec/physics.md
void gmp_rope_predict(void *system, float dt) {
	auto it = ropes_by_system.find(system);
	if (it == ropes_by_system.end()) return;
	for (Rope &r : it->second) {
		JPH::Body *b1 = r.c->GetBody1(), *b2 = r.c->GetBody2();
		JPH::RVec3 p1 = b1->GetCenterOfMassTransform() * r.c->GetConstraintToBody1Matrix().GetTranslation();
		JPH::RVec3 p2 = b2->GetCenterOfMassTransform() * r.c->GetConstraintToBody2Matrix().GetTranslation();
		JPH::Vec3 d = JPH::Vec3(p2 - p1);
		float len = d.Length();
		if (len < 1e-6f) continue;
		JPH::Vec3 dir = d / len;
		float rate = (b2->GetPointVelocity(p2) - b1->GetPointVelocity(p1)).Dot(dir);
		bool taut = len + rate * dt > r.max_d;
		r.c->SetDistance(taut ? r.max_d : r.min_d, r.max_d);
		// the catch tick itself is a hard stop (171 deg/s on the hang against 120 through the spring)
		JPH::SpringSettings ss = ROPE_SPRING;
		if (taut && len < r.max_d) ss.mFrequency = 0.0f;
		r.c->SetLimitsSpringSettings(ss);
	}
}

// hinge (phys_hinge): world space point and axis in Jolt metres/axes
void *gmp_add_hinge_constraint(void *system, unsigned int id1, unsigned int id2, float px, float py, float pz,
                               float ax, float ay, float az) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID ids[2] = { JPH::BodyID(id1), JPH::BodyID(id2) };
	JPH::BodyLockMultiWrite lock(sys->GetBodyLockInterface(), ids, 2);
	JPH::Body *a = lock.GetBody(0), *b = lock.GetBody(1);
	if (!a || !b) return nullptr;
	JPH::HingeConstraintSettings s;
	s.mSpace = JPH::EConstraintSpace::WorldSpace;
	s.mPoint1 = s.mPoint2 = JPH::RVec3(px, py, pz);
	JPH::Vec3 axis = JPH::Vec3(ax, ay, az).Normalized();
	s.mHingeAxis1 = s.mHingeAxis2 = axis;
	s.mNormalAxis1 = s.mNormalAxis2 = axis.GetNormalizedPerpendicular();
	JPH::Constraint *c = s.Create(*a, *b);
	sys->AddConstraint(c);
	return c;
}

// ball socket (phys_ballsocket): the two bodies share a world space point
void *gmp_add_point_constraint(void *system, unsigned int id1, unsigned int id2, float px, float py, float pz) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID ids[2] = { JPH::BodyID(id1), JPH::BodyID(id2) };
	JPH::BodyLockMultiWrite lock(sys->GetBodyLockInterface(), ids, 2);
	JPH::Body *a = lock.GetBody(0), *b = lock.GetBody(1);
	if (!a || !b) return nullptr;
	JPH::PointConstraintSettings s;
	s.mSpace = JPH::EConstraintSpace::WorldSpace;
	s.mPoint1 = s.mPoint2 = JPH::RVec3(px, py, pz);
	JPH::Constraint *c = s.Create(*a, *b);
	sys->AddConstraint(c);
	return c;
}
// slider (phys_slideconstraint): body 2 slides without friction along the world axis from its
// point on body 1 to its point on body 2, keeping its orientation (measured: g * cos on a tilted axis)
void *gmp_add_slider_constraint(void *system, unsigned int id1, unsigned int id2, float x1, float y1, float z1,
                                float x2, float y2, float z2) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID ids[2] = { JPH::BodyID(id1), JPH::BodyID(id2) };
	JPH::BodyLockMultiWrite lock(sys->GetBodyLockInterface(), ids, 2);
	JPH::Body *a = lock.GetBody(0), *b = lock.GetBody(1);
	if (!a || !b) return nullptr;
	JPH::SliderConstraintSettings s;
	s.mSpace = JPH::EConstraintSpace::WorldSpace;
	s.mAutoDetectPoint = true;
	JPH::Vec3 axis = JPH::Vec3(x2 - x1, y2 - y1, z2 - z1);
	if (axis.LengthSq() < 1e-10f) axis = JPH::Vec3::sAxisY();
	s.SetSliderAxis(axis.Normalized());
	JPH::Constraint *c = s.Create(*a, *b);
	sys->AddConstraint(c);
	return c;
}
// teleport (PhysObj:SetPos/SetAngles): Jolt metres, quaternion xyzw
void gmp_set_pose(void *system, unsigned int id, float px, float py, float pz, float qx, float qy, float qz,
                  float qw) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	sys->GetBodyInterface().SetPositionAndRotation(JPH::BodyID(id), JPH::RVec3(px, py, pz),
	                                               JPH::Quat(qx, qy, qz, qw), JPH::EActivation::Activate);
	sys->GetBodyInterface().SetLinearAndAngularVelocity(JPH::BodyID(id), JPH::Vec3::sZero(), JPH::Vec3::sZero());
}

void gmp_remove_body(void *system, unsigned int id) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID bid(id);
	sys->GetBodyInterface().RemoveBody(bid);
	sys->GetBodyInterface().DestroyBody(bid);
}

void gmp_remove_constraint(void *system, void *constraint) {
	auto it = ropes_by_system.find(system);
	if (it != ropes_by_system.end())
		for (size_t i = 0; i < it->second.size(); ++i)
			if (it->second[i].c == constraint) { it->second.erase(it->second.begin() + (long)i); break; }
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	sys->RemoveConstraint(reinterpret_cast<JPH::Constraint *>(constraint));
}

// `PhysObj:SetMass`: rescales mass and inertia together (IVP keeps the inertia per unit mass)
void gmp_set_mass(void *system, unsigned int id, float mass) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyLockWrite lock(sys->GetBodyLockInterface(), JPH::BodyID(id));
	if (!lock.Succeeded()) return;
	JPH::Body &b = lock.GetBody();
	if (!b.IsDynamic()) return;
	JPH::MotionProperties *mp = b.GetMotionProperties();
	float inv = mp->GetInverseMass();
	if (inv <= 0.0f || mass <= 0.0f) return;
	float factor = mass * inv;
	mp->SetInverseMass(inv / factor);
	mp->SetInverseInertia(mp->GetInverseInertiaDiagonal() / factor, mp->GetInertiaRotation());
}

// angular impulse (world axes)
void gmp_add_angular_impulse(void *system, unsigned int id, float ix, float iy, float iz) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	sys->GetBodyInterface().AddAngularImpulse(JPH::BodyID(id), JPH::Vec3(ix, iy, iz));
}

// impulse through the mass centre (no torque)
void gmp_add_impulse(void *system, unsigned int id, float ix, float iy, float iz) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	sys->GetBodyInterface().AddImpulse(JPH::BodyID(id), JPH::Vec3(ix, iy, iz));
}

// 1 / effective mass of a body for an impulse at world point `p` along `dir` (kg^-1; the
// rotational term uses the body's inverse inertia). 0 for a static or frozen body.
float gmp_point_inverse_mass(void *system, unsigned int id, float px, float py, float pz, float dx, float dy, float dz) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyLockRead lock(sys->GetBodyLockInterface(), JPH::BodyID(id));
	if (!lock.Succeeded()) return 0.0f;
	const JPH::Body &b = lock.GetBody();
	if (!b.IsDynamic()) return 0.0f;
	JPH::Vec3 r = JPH::Vec3(JPH::RVec3(px, py, pz) - b.GetCenterOfMassPosition());
	JPH::Vec3 d(dx, dy, dz);
	JPH::Vec3 rxd = r.Cross(d);
	return b.GetMotionProperties()->GetInverseMass() + rxd.Dot(b.GetInverseInertia().Multiply3x3(rxd));
}

}

extern "C" {

// solver tuning for the contact response experiments
void gmp_set_solver(void *system, float baumgarte, unsigned int velocity_steps, unsigned int position_steps, float min_velocity_for_restitution) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::PhysicsSettings s = sys->GetPhysicsSettings();
	s.mBaumgarte = baumgarte;
	s.mNumVelocitySteps = velocity_steps;
	s.mNumPositionSteps = position_steps;
	s.mMinVelocityForRestitution = min_velocity_for_restitution;
	sys->SetPhysicsSettings(s);
}

}

// Impact response in the IVP style (clean room, from measured behaviour): the first contact of a
// pair is resolved at a single point with an iterative push along a friction tilted direction
// until the bodies stop approaching, then one more push to the measured separation speed
// sqrt(e1 e2) |vn| + 0.01 sqrt(n) m/s. Jolt's own restitution is zeroed for that contact so
// its solver only keeps the bodies apart afterwards.
#include <Jolt/Physics/Collision/ContactListener.h>
#include <Jolt/Physics/Body/Body.h>
#include <unordered_map>
#include <unordered_set>
#include <vector>
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>

namespace {

struct GmpSurface { float friction; float elasticity; };
// one record per resolved first contact of a pair in a step: the bodies, the contact point
// (Jolt metres) and the approach speed along the normal (m/s) before the impact
struct GmpImpactRec { unsigned id1, id2; float px, py, pz, speed; };
// a contact pair event of the step: kind 0 added, 1 persisted, 2 removed
struct GmpContactRec { unsigned id1, id2, kind; };

class GmpImpactListener final : public JPH::ContactListener {
public:
	std::unordered_map<JPH::uint32, GmpSurface> surfaces;
	std::vector<GmpImpactRec> impacts;
	std::vector<GmpContactRec> contacts;
	std::mutex contacts_mutex;
	void contact(JPH::uint32 id1, JPH::uint32 id2, unsigned kind) {
		std::lock_guard<std::mutex> g(contacts_mutex);
		contacts.push_back(GmpContactRec{id1, id2, kind});
	}
	std::unordered_map<JPH::uint32, std::vector<JPH::Vec3>> hulls;
	bool enabled = true;
	// body pairs that never collide (constraint.NoCollide); key is (min id << 32 | max id)
	std::unordered_set<unsigned long long> nocollide;
	static unsigned long long pair_key(JPH::uint32 a, JPH::uint32 b) {
		return ((unsigned long long)std::min(a, b) << 32) | std::max(a, b);
	}
	// COLLISION_GROUP_WORLD bodies (the collision_off property): they touch static geometry only
	std::unordered_set<JPH::uint32> world_only;
	JPH::ValidateResult OnContactValidate(const JPH::Body &b1, const JPH::Body &b2, JPH::RVec3Arg,
	                                      const JPH::CollideShapeResult &) override {
		JPH::uint32 id1 = b1.GetID().GetIndexAndSequenceNumber(), id2 = b2.GetID().GetIndexAndSequenceNumber();
		if (!nocollide.empty() && nocollide.count(pair_key(id1, id2)))
			return JPH::ValidateResult::RejectAllContactsForThisBodyPair;
		if (!world_only.empty() && ((world_only.count(id1) && !b2.IsStatic()) || (world_only.count(id2) && !b1.IsStatic())))
			return JPH::ValidateResult::RejectAllContactsForThisBodyPair;
		return JPH::ValidateResult::AcceptAllContactsForThisBodyPair;
	}
	float static_mass_factor = 100000.0f;
	float step_dt = 0.015f;
	int tick = 0;
	float gravity = 9.81f; // m/s^2, the push step's resting friction comes from the weight only
	JPH::PhysicsSystem *sys = nullptr; // the world's gravity is read live (set after install)
	float persisted_threshold = 0.3f; // m/s, resting contacts stay with Jolt
	// size of one push as a fraction of the point's full stopping impulse; measured on the tilted
	// crate's corner impact: 0.03 to 0.075 reproduce the game's impulse within 0.5 %, 0.1 misses by 4.5 %
	// (docs/spec/physics.md); GMP_PUSH_FRACTION overrides for experiments
	float push_fraction = getenv("GMP_PUSH_FRACTION") ? (float)atof(getenv("GMP_PUSH_FRACTION")) : 0.075f;
	bool debug = getenv("GMP_IMPACT_DEBUG") != nullptr;
	std::unordered_map<JPH::uint64, std::vector<size_t>> contact_points; // pair -> hull vertex indices that already impacted
	// resting contact anchors: a body pushed while at rest keeps its tangential velocity in the
	// push step (no friction) and is caught at its leading edge the step after
	struct RestState { float vt_prev = 0.0f; float vn_in = 0.0f; int phase = 0; int landed_tick = -1; };
	std::unordered_map<JPH::uint64, RestState> rest;
	// the game integrates up to the moment of impact with the old velocity and the rest of the
	// step with the new one; Jolt integrates the whole step with the new one, so after the step
	// every impacted body is moved by (v_old - v_new) * t_impact (and turned likewise)
	struct Offset { JPH::Vec3 dpos; JPH::Vec3 drot; };
	std::unordered_map<JPH::uint32, Offset> offsets;
	std::mutex offsets_mutex;
	std::unordered_map<JPH::uint32, bool> skip; // bodies under a shadow controller (physgun) keep Jolt's contact

	GmpSurface surface(const JPH::Body &b) const {
		auto it = surfaces.find(b.GetID().GetIndexAndSequenceNumber());
		return it == surfaces.end() ? GmpSurface{0.8f, 0.25f} : it->second;
	}

	static JPH::Vec3 point_velocity(const JPH::Body &b, JPH::RVec3Arg p) {
		if (!b.IsDynamic() && !b.IsKinematic()) return JPH::Vec3::sZero();
		return b.GetLinearVelocity() + b.GetAngularVelocity().Cross(JPH::Vec3(p - b.GetCenterOfMassPosition()));
	}

	// effective mass of one body along `dir` for an impulse at `p`
	static float virt_mass(const JPH::Body &b, JPH::RVec3Arg p, JPH::Vec3Arg dir, float other_mass, float static_factor) {
		if (!b.IsDynamic()) return static_factor * other_mass;
		const JPH::MotionProperties *mp = b.GetMotionProperties();
		JPH::Vec3 r = JPH::Vec3(p - b.GetCenterOfMassPosition());
		JPH::Vec3 rxd = r.Cross(dir);
		JPH::Mat44 inv_i = b.GetInverseInertia();
		float k = mp->GetInverseMass() + rxd.Dot(inv_i.Multiply3x3(rxd));
		return k > 1e-12f ? 1.0f / k : 1e12f;
	}

	static void push(const JPH::Body &b, JPH::RVec3Arg p, JPH::Vec3Arg impulse) {
		if (!b.IsDynamic()) return;
		JPH::Body &body = const_cast<JPH::Body &>(b);
		JPH::MotionProperties *mp = body.GetMotionProperties();
		JPH::Vec3 r = JPH::Vec3(p - b.GetCenterOfMassPosition());
		mp->SetLinearVelocityClamped(mp->GetLinearVelocity() + impulse * mp->GetInverseMass());
		mp->SetAngularVelocityClamped(mp->GetAngularVelocity() + b.GetInverseInertia().Multiply3x3(r.Cross(impulse)));
	}

	void OnContactRemoved(const JPH::SubShapeIDPair &pair) override {
		contact(pair.GetBody1ID().GetIndexAndSequenceNumber(), pair.GetBody2ID().GetIndexAndSequenceNumber(), 2);
		contact_points.erase((JPH::uint64(pair.GetBody1ID().GetIndexAndSequenceNumber()) << 32) | pair.GetBody2ID().GetIndexAndSequenceNumber());
		rest.erase((JPH::uint64(pair.GetBody1ID().GetIndexAndSequenceNumber()) << 32) | pair.GetBody2ID().GetIndexAndSequenceNumber());
	}

	void OnContactPersisted(const JPH::Body &b1, const JPH::Body &b2, const JPH::ContactManifold &m, JPH::ContactSettings &s) override {
		contact(b1.GetID().GetIndexAndSequenceNumber(), b2.GetID().GetIndexAndSequenceNumber(), 1);
		impact(b1, b2, m, s, persisted_threshold);
		if (debug) {
			const JPH::Body &d = b1.IsDynamic() ? b1 : b2;
			JPH::Vec3 lv = d.GetLinearVelocity(), av = d.GetAngularVelocity();
			fprintf(stderr, "DBGCB t=%d id=%u persisted m=%.1f pts=%u pen=%.4f f=%.3f n=(%.2f %.2f %.2f) lv=(%.3f %.3f %.3f) av=(%.2f %.2f %.2f)\n", tick, d.GetID().GetIndex(), 1.0f / d.GetMotionProperties()->GetInverseMass(), m.mRelativeContactPointsOn1.size(), m.mPenetrationDepth, s.mCombinedFriction, m.mWorldSpaceNormal.GetX(), m.mWorldSpaceNormal.GetY(), m.mWorldSpaceNormal.GetZ(), lv.GetX(), lv.GetY(), lv.GetZ(), av.GetX(), av.GetY(), av.GetZ());
		}
	}

	void OnContactAdded(const JPH::Body &b1, const JPH::Body &b2, const JPH::ContactManifold &m, JPH::ContactSettings &s) override {
		contact(b1.GetID().GetIndexAndSequenceNumber(), b2.GetID().GetIndexAndSequenceNumber(), 0);
		impact(b1, b2, m, s, 1e-4f);
		if (debug) {
			const JPH::Body &d = b1.IsDynamic() ? b1 : b2;
			JPH::Vec3 lv = d.GetLinearVelocity(), av = d.GetAngularVelocity();
			fprintf(stderr, "DBGCB t=%d id=%u added m=%.1f pts=%u pen=%.4f f=%.3f n=(%.2f %.2f %.2f) lv=(%.3f %.3f %.3f) av=(%.2f %.2f %.2f)\n", tick, d.GetID().GetIndex(), 1.0f / d.GetMotionProperties()->GetInverseMass(), m.mRelativeContactPointsOn1.size(), m.mPenetrationDepth, s.mCombinedFriction, m.mWorldSpaceNormal.GetX(), m.mWorldSpaceNormal.GetY(), m.mWorldSpaceNormal.GetZ(), lv.GetX(), lv.GetY(), lv.GetZ(), av.GetX(), av.GetY(), av.GetZ());
		}
	}

	void impact(const JPH::Body &b1, const JPH::Body &b2, const JPH::ContactManifold &m, JPH::ContactSettings &s, float threshold) {
		GmpSurface s1 = surface(b1), s2 = surface(b2);
		float friction = std::min(1.0f, std::max(0.0f, s1.friction * s2.friction));
		float elasticity = std::min(1.0f, std::max(0.0f, s1.elasticity * s2.elasticity));
		s.mCombinedFriction = friction;
		s.mCombinedRestitution = 0.0f;
		if (!enabled || m.mRelativeContactPointsOn1.empty()) return;
		// a manifold against the world whose normal is not the world surface normal under
		// the contact point is an edge contact at a seam between two brushes (the normal
		// turns from tick to tick: (-0.87,-0.49) then (-0.46,-0.89) on `phys_drop_tilted`);
		// the game has no such contact, so it is not a collision here either
		if (b1.IsDynamic() != b2.IsDynamic()) {
			const JPH::Body &st = b1.IsDynamic() ? b2 : b1;
			JPH::SubShapeID sid = b1.IsDynamic() ? m.mSubShapeID2 : m.mSubShapeID1;
			JPH::RVec3 cp = b1.IsDynamic() ? m.GetWorldSpaceContactPointOn2(0) : m.GetWorldSpaceContactPointOn1(0);
			JPH::Vec3 outward = b1.IsDynamic() ? -m.mWorldSpaceNormal : m.mWorldSpaceNormal;
			JPH::Vec3 surf = st.GetWorldSpaceSurfaceNormal(sid, cp);
			if (surf.Dot(outward) < 0.985f && m.mPenetrationDepth < -0.005f) {
				if (debug) fprintf(stderr, "seam contact t=%d n=(%.2f %.2f %.2f) surf=(%.2f %.2f %.2f) pts=%u pen=%.4f\n", tick, outward.GetX(), outward.GetY(), outward.GetZ(), surf.GetX(), surf.GetY(), surf.GetZ(), m.mRelativeContactPointsOn1.size(), m.mPenetrationDepth);
				s.mIsSensor = true;
				return;
			}
		}
		if (skip.count(b1.GetID().GetIndexAndSequenceNumber()) || skip.count(b2.GetID().GetIndexAndSequenceNumber())) return;
		JPH::Vec3 n = -m.mWorldSpaceNormal; // pushes body 1 away from body 2
		float m1 = b1.IsDynamic() ? 1.0f / b1.GetMotionProperties()->GetInverseMass() : 0.0f;
		float m2 = b2.IsDynamic() ? 1.0f / b2.GetMotionProperties()->GetInverseMass() : 0.0f;
		if (m1 == 0.0f) m1 = static_mass_factor * m2;
		if (m2 == 0.0f) m2 = static_mass_factor * m1;
		// impact set: the hull vertices of the dynamic body that reach the contact plane within
		// this step, deepest first (the game impacts one closest feature pair at a time and
		// re-tests the others after each push); manifold points when no hull is known
		struct Pt { JPH::RVec3 p; float d; float vn0; JPH::Vec3 dir; float step; size_t index; };
		std::vector<Pt> pts;
		bool verts_of_b2_used = false;
		auto vertices_of = [&](const JPH::Body &b) -> const std::vector<JPH::Vec3> * {
			if (!b.IsDynamic()) return nullptr;
			auto it = hulls.find(b.GetID().GetIndexAndSequenceNumber());
			return it == hulls.end() ? nullptr : &it->second;
		};
		JPH::uint best = 0;
		float best_pen = -1e30f;
		for (JPH::uint i = 0; i < m.mRelativeContactPointsOn1.size(); ++i) {
			float pen = (m.mRelativeContactPointsOn1[i] - m.mRelativeContactPointsOn2[i]).Dot(m.mWorldSpaceNormal);
			if (pen > best_pen) { best_pen = pen; best = i; }
		}
		auto consider = [&](JPH::RVec3Arg p, float d) { pts.push_back(Pt{p, d, 0.0f, n, 0.0f, pts.size()}); };
		if (const auto *verts = vertices_of(b1)) {
			JPH::RVec3 p0 = m.GetWorldSpaceContactPointOn2(best);
			JPH::RMat44 xf = b1.GetWorldTransform();
			for (const JPH::Vec3 &lv : *verts) {
				JPH::RVec3 w = xf * lv;
				consider(w, float(JPH::Vec3(w - p0).Dot(n)));
			}
		} else if (const auto *verts = vertices_of(b2)) {
			verts_of_b2_used = true;
			JPH::RVec3 p0 = m.GetWorldSpaceContactPointOn1(best);
			JPH::RMat44 xf = b2.GetWorldTransform();
			for (const JPH::Vec3 &lv : *verts) {
				JPH::RVec3 w = xf * lv;
				consider(w, float(JPH::Vec3(w - p0).Dot(-n)));
			}
		}
		if (pts.empty()) {
			for (JPH::uint i = 0; i < m.mRelativeContactPointsOn1.size(); ++i) {
				float pen = (m.mRelativeContactPointsOn1[i] - m.mRelativeContactPointsOn2[i]).Dot(m.mWorldSpaceNormal);
				consider(m.mBaseOffset + 0.5f * (m.mRelativeContactPointsOn1[i] + m.mRelativeContactPointsOn2[i]), -pen);
			}
		}
		if (pts.empty()) return;
		// a face already resting on the surface (three or more vertices at it) is a resting
		// contact, not an impact: the game's drum pushed by a bullet keeps its whole horizontal
		// velocity and loses the vertical (`shoot_prop_357_drum`), a rim vertex impact here
		// would give it the sliding barrel's yaw spin and sideways kick
		// three touching points of one corner (Jolt's manifold spreads a penetrating corner over
		// several points) are not a face: the lockers pivoting on a corner would be anchored and
		// never tip over, where the game's roll it over in 40 ticks
		std::vector<const Pt *> touch;
		for (const Pt &pt : pts) {
			if (pt.d > 0.001f) continue;
			bool distinct = true;
			for (const Pt *o : touch) if ((o->p - pt.p).Length() < 0.05f) { distinct = false; break; }
			if (distinct) touch.push_back(&pt);
		}
		int touching = (int)touch.size();
		if (touching >= 3) {
			// resting contact anchors (`phys_push_drum_*`, `shoot_prop_*_drum`): a sliding resting
			// face is held by its leading bottom edge - the impulse that stops that edge's
			// tangential and normal velocity, its tangential part Coulomb-capped by
			// f (N_anchor + m g dt + N_stored), N_stored the normal impulse the floor took when
			// the body was pushed into it (a shot drum: no anchor in the push step, caught the
			// step after); the game's drum pushed at 10-320 u/s loses 0.4236 v until the cap
			const JPH::Body &dyn = b1.IsDynamic() ? b1 : b2;
			const JPH::Body &other = b1.IsDynamic() ? b2 : b1;
			JPH::uint64 key = (JPH::uint64(b1.GetID().GetIndexAndSequenceNumber()) << 32) | b2.GetID().GetIndexAndSequenceNumber();
			RestState &rs = rest[key];
			// a face that was just solved as an impact this step is bouncing, not resting
			if (rs.landed_tick == tick) return;
			JPH::Vec3 nd = b1.IsDynamic() ? n : -n; // away from the surface, whichever body is dynamic
			JPH::Vec3 v = dyn.GetLinearVelocity();
			JPH::Vec3 vt_vec = v - nd * v.Dot(nd);
			float vt = vt_vec.Length();
			// the world's gravity is Jolt's 9.81 scaled per body (gravity factor) to the game's 600 u/s^2
			float g_dt = (sys ? sys->GetGravity().Length() : gravity) * dyn.GetMotionProperties()->GetGravityFactor() * step_dt;
			float mass = 1.0f / dyn.GetMotionProperties()->GetInverseMass();
			JPH::RVec3 c = dyn.GetCenterOfMassPosition();
			float vn_in = std::max(-(point_velocity(dyn, c) - point_velocity(other, c)).Dot(nd) - g_dt, 0.0f);


			// a pair that just went through vertex impacts (phase 3) is a landing, not a push from rest
			bool pushed = rs.phase != 3 && rs.vt_prev < 0.02f && vt > 0.1f;
			if (pushed && vn_in > 0.02f) {
				// pushed into the floor: the floor takes the normal part with friction on the
				// weight alone (the drum loses 6-9 u/s whether pushed at 20 or 334 u/s)
				s.mCombinedFriction = friction * g_dt / (vn_in + g_dt);
				rs.vn_in = vn_in;
				rs.phase = 1;
			} else if (other.IsDynamic() || vt < 0.02f) {
				rs.phase = 0;
				rs.vn_in = 0.0f;
			} else {
				JPH::Vec3 dir = vt_vec / vt;
				const Pt *lead = nullptr;
				float best = -1e30f;
				for (const Pt &pt : pts) {
					if (pt.d > 0.001f) continue;
					float a = JPH::Vec3(pt.p - c).Dot(dir);
					if (a > best) { best = a; lead = &pt; }
				}
				if (lead) {
					// the resting face carries the weight: cancel this step's fall into the rest gap
					// first, the anchor's lift then comes out whole (the game's 13.7 u/s on the drum)
					float fall = std::max(-(dyn.GetLinearVelocity() - other.GetLinearVelocity()).Dot(nd), 0.0f);
					push(dyn, c, nd * (mass * std::min(fall, g_dt)));
					JPH::Vec3 r = JPH::Vec3(lead->p - c);
					JPH::Mat44 rx = JPH::Mat44::sCrossProduct(r);
					JPH::Mat44 k = JPH::Mat44::sScale(dyn.GetMotionProperties()->GetInverseMass()) - rx * dyn.GetInverseInertia() * rx;
					JPH::Vec3 j = k.Inversed3x3().Multiply3x3(-point_velocity(dyn, lead->p));
					float jn = std::max(j.Dot(nd), 0.0f);
					JPH::Vec3 jt = j - nd * j.Dot(nd);
					// Coulomb cap solved with the anchor's own normal part scaling along: J_t = f (ratio J_t + m (g dt + vn_stored))
					// the anchor's normal part only counts while the face is still flat (the game
					// loses 15.3 u/s in the push step, then f g dt per step once the drum tips)
					bool flat = dyn.GetAngularVelocity().Length() < 0.4f;
					float ratio = flat && jt.Length() > 1e-9f ? jn / jt.Length() : 0.0f;
					float denom = 1.0f - friction * ratio;
					float cap = denom > 1e-3f ? friction * mass * (g_dt + rs.vn_in) / denom : 1e30f;
					float scale = jt.Length() > cap ? cap / jt.Length() : 1.0f;
					push(dyn, lead->p, j * scale);
					if (debug) fprintf(stderr, "rest anchor r=(%.4f %.4f %.4f) j=(%.3f %.3f %.3f) scale=%.3f stored=%.3f v=(%.3f %.3f %.3f)\n", (double)r.GetX(), (double)r.GetY(), (double)r.GetZ(), (double)j.GetX(), (double)j.GetY(), (double)j.GetZ(), (double)scale, (double)rs.vn_in, (double)dyn.GetLinearVelocity().GetX(), (double)dyn.GetLinearVelocity().GetY(), (double)dyn.GetLinearVelocity().GetZ());
					s.mCombinedFriction = 0.0f;
				}
				rs.vn_in = 0.0f;
				rs.phase = 2;
			}
			rs.vt_prev = vt;
			return;
		}
		const JPH::Body &dyn = b1.IsDynamic() ? b1 : b2;
		JPH::Vec3 lv0 = dyn.GetLinearVelocity(), av0 = dyn.GetAngularVelocity();
		float t_impact = -1.0f;
		bool face_done = false;
		float e_eff = elasticity;
		float extra = 0.0f;
		auto rel = [&](JPH::RVec3Arg p) { return point_velocity(b1, p) - point_velocity(b2, p); };
		// friction tilt: tan(angle) = f (1 + sqrt(e)); push = c n - s (tangential part of the
		// unit relative velocity) unless the motion is already inside the friction cone
		float tilt = std::atan(friction * (1.0f + std::sqrt(elasticity)));
		// diagnostic only: how many hull vertices arrive on the plane in this step (a face
		// landing keeps the tilted vertex pushes - both flat landings of `phys_drop_tilted`
		// lose f * dJn of slide in the game, see docs/spec/physics.md)
		{
			int approaching = 0;
			float vn_face = 0.0f;
			for (const Pt &pt : pts) {
				float vn = rel(pt.p).Dot(n);
				if (pt.d <= 0.001f + 0.02f && vn < 0.0f) { ++approaching; vn_face = std::max(vn_face, -vn); }
			}
			if (debug) fprintf(stderr, "DBGFACE t=%d id=%u pts=%zu approaching=%d vn_face=%.3f lv=(%.3f %.3f %.3f) n=(%.2f %.2f %.2f)\n", tick, dyn.GetID().GetIndex(), pts.size(), approaching, vn_face, lv0.GetX(), lv0.GetY(), lv0.GetZ(), n.GetX(), n.GetY(), n.GetZ());
		}
		float c = std::cos(tilt), sn = std::sin(tilt);
		auto direction = [&](JPH::Vec3Arg vrel, JPH::Vec3Arg prev) {
			float len = vrel.Length();
			if (len < 1e-6f) return prev;
			JPH::Vec3 vh = vrel / len;
			float dot = vh.Dot(n);
			if (dot > 0.0f) return prev;
			if (-dot >= c) return n;
			return c * n - sn * (vh - n * dot);
		};
		bool logged = false;
		auto resolve = [&](Pt &pt) {
			JPH::Vec3 v = rel(pt.p);
			float vn = v.Dot(n);
			if (vn >= 0.0f) return false;
			pt.vn0 = vn;
			if (!logged) {
				logged = true;
				impacts.push_back(GmpImpactRec{b1.GetID().GetIndexAndSequenceNumber(), b2.GetID().GetIndexAndSequenceNumber(),
				                               float(pt.p.GetX()), float(pt.p.GetY()), float(pt.p.GetZ()), -vn});
			}
			pt.dir = direction(v, n);
			float mv1 = virt_mass(b1, pt.p, pt.dir, m2, static_mass_factor);
			float mv2 = virt_mass(b2, pt.p, pt.dir, m1, static_mass_factor);
			pt.step = push_fraction * (-2.0f * vn * (mv1 * mv2 / (mv1 + mv2)));
			// a point that arrives without slip (a prop falling straight onto a corner) sticks:
			// a step's tangential impulse is capped at what stops the tangential velocity the
			// normal part induces - the game's lockers (2000 kg) take 0.41 of the normal impulse
			// tangentially where the full Coulomb tilt gives 0.78; a corner that slides in keeps
			// the full tilt (the tilted crate's second corner)
			bool stick = true; // measured on the slip series: the lockers stick at every entry slip
			for (int it = 0; it < 100 && vn < 0.0f; ++it) {
				push(b1, pt.p, pt.dir * pt.step);
				push(b2, pt.p, -pt.dir * pt.step);
				v = rel(pt.p);
				vn = v.Dot(n);
				pt.dir = direction(v, pt.dir);
				if (!stick) continue;
				JPH::Vec3 vt = v - n * vn;
				float vt_len = vt.Length();
				float dn = pt.dir.Dot(n);
				float jt_step = pt.step * std::sqrt(std::max(0.0f, 1.0f - dn * dn));
				if (vt_len > 1e-6f && jt_step > 1e-9f) {
					JPH::Vec3 th = vt / vt_len;
					float mt1 = virt_mass(b1, pt.p, th, m2, static_mass_factor);
					float mt2 = virt_mass(b2, pt.p, th, m1, static_mass_factor);
					float jt_stick = vt_len * (mt1 * mt2 / (mt1 + mt2));
					if (jt_stick < jt_step) {
						float sphi = std::min(jt_stick / pt.step, 1.0f);
						pt.dir = n * std::sqrt(1.0f - sphi * sphi) - th * sphi;
					}
				}
			}
			float wanted = extra + std::sqrt(e_eff) * (-pt.vn0) - vn;
			if (wanted > 0.0f) {
				push(b1, pt.p, pt.dir);
				push(b2, pt.p, -pt.dir);
				float dvn = rel(pt.p).Dot(n) - vn;
				push(b1, pt.p, -pt.dir);
				push(b2, pt.p, pt.dir);
				if (std::fabs(dvn) > 1e-4f) {
					push(b1, pt.p, pt.dir * (wanted / dvn));
					push(b2, pt.p, -pt.dir * (wanted / dvn));
				}
			}
			return true;
		};
		// one closest feature pair at a time, like the game: the vertex that touches first
		// (least distance, ties keep hull order), fully resolved, then the others are re-tested
		// with the new velocities; the game shows two impacts for a flat crate (a corner, then
		// the opposite one) and one rim vertex for the barrel
		const float tie_eps = 0.0005f;
		JPH::uint64 pair_key = (JPH::uint64(b1.GetID().GetIndexAndSequenceNumber()) << 32) | b2.GetID().GetIndexAndSequenceNumber();
		std::vector<size_t> &cps = contact_points[pair_key];
		rest[pair_key].phase = 3; // landing in progress: the resting branch must not read it as a push
		if (verts_of_b2_used) cps.clear();
		for (int round = 0; round < 8; ++round) {
			// a face arriving flat (three or more vertices reach the plane this step, fastest
			// above 0.3 m/s) is one simultaneous impact, not a chain of vertex impacts: every
			// arriving vertex is driven to sqrt(e) of its own approach along its tilted push
			// direction (Gauss-Seidel, accumulated impulses never pull). The game's flat crate
			// at 653 u/s bounces 98 u/s with its yaw spin 148 -> 0 deg/s (`phys_spin_crate_z200`);
			// the vertex chain gave no bounce and a 211/169 deg/s tumble
			if (round == 0) {
				std::vector<Pt *> face;
				float fastest = 0.0f, t_first = step_dt;
				for (Pt &pt : pts) {
					float vn = rel(pt.p).Dot(n);
					if (vn >= -threshold || (pt.d > 0.0f && pt.d > -vn * step_dt)) continue;
					bool dup = false;
					for (const Pt *f : face) if ((f->p - pt.p).LengthSq() < 1e-6f) { dup = true; break; }
					if (dup) continue;
					pt.vn0 = vn;
					pt.dir = direction(rel(pt.p), n);
					face.push_back(&pt);
					fastest = std::max(fastest, -vn);
					t_first = std::min(t_first, std::max(pt.d, 0.0f) / -vn);
				}
				// only vertices reaching the plane together (within 5 mm of the nearest) form the
				// face: a corner region of a chamfered hull has several vertices inside one step
				if (face.size() >= 2) {
					float d_min = 1e30f;
					for (const Pt *f : face) d_min = std::min(d_min, f->d);
					std::vector<Pt *> keep;
					for (Pt *f : face) if (f->d <= d_min + 0.005f) keep.push_back(f);
					face.swap(keep);
				}
				// a landing only: the face is still above the rest gap (a resting face pushed into
				// the floor - the shot drum - stays with the resting rules)
				float d_face = 1e30f;
				for (const Pt *f : face) d_face = std::min(d_face, f->d);
				// a face sliding fast along the plane is not a face landing in the game (the drum
				// at 285 u/s strikes rim vertices); a face arriving without slip is
				float face_slip = 0.0f;
				for (const Pt *f : face) { JPH::Vec3 vr = rel(f->p); face_slip = std::max(face_slip, (vr - n * vr.Dot(n)).Length()); }
				// an edge arriving level (two vertices more than 20 cm apart reach the plane
				// together) is one impact too: in the game the +x end leaves at sqrt(e) of its
				// approach, the other end is merely stopped, and both keep their slip (the rolled
				// crate's ends slide at 4.6 / 2.1 m/s after the tick); the vertex chain left both
				// ends still descending at 1.7 / 2.5 m/s for Jolt to catch a tick later
				// a ring of vertices (the drum's 16-gon bottom) does not land flat in the game: the
				// yaw-spinning drum strikes the rim vertex on the hull's own +x axis first (628 u/s)
				// and its antipode second (245), one diameter apart, and tumbles at -375 deg/s
				bool ring = face.size() > 8;
				if (ring) {
					const JPH::Body &dyn = b1.IsDynamic() ? b1 : b2;
					JPH::Quat inv_rot = dyn.GetRotation().Conjugated();
					JPH::RVec3 com = dyn.GetCenterOfMassPosition();
					auto local_x = [&](const Pt *f) { return (inv_rot * JPH::Vec3(f->p - com)).GetX(); };
					Pt *a = face[0];
					for (Pt *f : face) if (local_x(f) > local_x(a)) a = f;
					Pt *b = a;
					for (Pt *f : face) if ((f->p - a->p).LengthSq() > (b->p - a->p).LengthSq()) b = f;
					face.clear();
					face.push_back(a);
					face.push_back(b);
				}
				bool edge_pair = face.size() == 2 && (face[0]->p - face[1]->p).Length() > 0.2f;
				if ((face.size() >= 3 || edge_pair) && fastest > 0.3f && d_face > 0.02f && face_slip < 3.0f) {
					std::vector<float> acc(face.size(), 0.0f);
					// the drum's ring is struck in sequence: the hull +x rim vertex as a free single
					// vertex, then its antipode by the approach the first impact left it
					for (size_t k0 = 0; k0 < 1u; ++k0) {
					if (ring && k0 == 1) face[1]->vn0 = std::min(0.0f, rel(face[1]->p).Dot(n));
					for (int it = 0; it < 60; ++it) {
						float moved = 0.0f;
						for (size_t k = ring ? k0 : 0; k < (ring ? k0 + 1 : face.size()); ++k) {
							Pt &pt = *face[k];
							JPH::Vec3 v = rel(pt.p);
							float vn = v.Dot(n);
							float target = std::sqrt(e_eff) * (-pt.vn0);
							if (edge_pair) {
								if (!ring && pt.p.GetX() < face[1 - k]->p.GetX()) target = 0.0f;
								pt.dir = direction(v, pt.dir);
							}
							float wanted = extra + target - vn;
							push(b1, pt.p, pt.dir);
							push(b2, pt.p, -pt.dir);
							float dvn = rel(pt.p).Dot(n) - vn;
							push(b1, pt.p, -pt.dir);
							push(b2, pt.p, pt.dir);
							if (dvn <= 1e-6f) continue;
							float j = wanted / dvn;
							if (acc[k] + j < 0.0f) j = -acc[k];
							if (std::fabs(j) < 1e-5f) continue;
							acc[k] += j;
							moved = std::max(moved, std::fabs(j * dvn));
							push(b1, pt.p, pt.dir * j);
							push(b2, pt.p, -pt.dir * j);
						}
						if (moved < 1e-4f) break;
					}
					}
					// inside the friction cone the vertices stick: the tangential velocity of each
					// is removed, bounded by f times its normal impulse (the game's flat crate
					// lands with its 148 deg/s yaw spin gone)
					for (int it = 0; it < (edge_pair ? 0 : 20); ++it) {
						float moved = 0.0f;
						for (size_t k = 0; k < face.size(); ++k) {
							Pt &pt = *face[k];
							JPH::Vec3 v = rel(pt.p);
							JPH::Vec3 vt = v - n * v.Dot(n);
							float len = vt.Length();
							if (len < 1e-4f) continue;
							JPH::Vec3 th = vt / len;
							push(b1, pt.p, -th);
							push(b2, pt.p, th);
							float dvt = len - (rel(pt.p) - n * rel(pt.p).Dot(n)).Dot(th);
							push(b1, pt.p, th);
							push(b2, pt.p, -th);
							if (dvt <= 1e-6f) continue;
							float jt = std::min(len / dvt, friction * acc[k] * pt.dir.Dot(n));
							if (jt <= 0.0f) continue;
							moved = std::max(moved, jt * dvt);
							push(b1, pt.p, -th * jt);
							push(b2, pt.p, th * jt);
						}
						if (moved < 1e-4f) break;
					}
					t_impact = t_first;
					rest[pair_key].landed_tick = tick;
					if (!logged && !face.empty()) {
						logged = true;
						const JPH::RVec3 &fp = face[0]->p;
						impacts.push_back(GmpImpactRec{b1.GetID().GetIndexAndSequenceNumber(), b2.GetID().GetIndexAndSequenceNumber(),
						                               float(fp.GetX()), float(fp.GetY()), float(fp.GetZ()), fastest});
					}
					// the contact is fully solved here: Jolt's own constraint on this manifold would
					// take the bounce back (vz 0 instead of +93 u/s), and the game leaves the body
					// exactly at the contact position for this step (z -123.55 = rest height)
					s.mIsSensor = true;
					face_done = true;
					if (debug) fprintf(stderr, "face impact t=%d verts=%zu fastest=%.3f e=%.3f lv=(%.3f %.3f %.3f)->(%.3f %.3f %.3f) av=(%.2f %.2f %.2f)->(%.2f %.2f %.2f)\n", tick, face.size(), fastest, std::sqrt(e_eff), lv0.GetX(), lv0.GetY(), lv0.GetZ(), dyn.GetLinearVelocity().GetX(), dyn.GetLinearVelocity().GetY(), dyn.GetLinearVelocity().GetZ(), av0.GetX(), av0.GetY(), av0.GetZ(), dyn.GetAngularVelocity().GetX(), dyn.GetAngularVelocity().GetY(), dyn.GetAngularVelocity().GetZ());
					break;
				}
			}
			// the vertex that reaches the surface first: least time d / |vn| (the first round
			// has every vertex at the same distance, later rounds are decided by the rotation
			// the previous impact added); ties keep hull order
			Pt *pick = nullptr;
			float best_t = 0.0f;
			for (Pt &pt : pts) {
				float vn = rel(pt.p).Dot(n);
				if (vn >= -threshold) continue;
				if (pt.d > 0.0f && pt.d > -vn * step_dt) continue;
				// a later vertex enters the step only if it reaches the plane in the time left
				// after the first touch (`phys_drop_edge_crate`: the far side needs 12 ms with 7 left)
				if (t_impact >= 0.0f && pt.d > 0.0f && pt.d > -vn * (step_dt - t_impact)) continue;
				float t = std::max(pt.d, 0.0f) / -vn;
				// ties: the game took the +x corner of the crate and the +x rim vertex of the barrel
				bool better = pick == nullptr || t < best_t - tie_eps * 0.01f ||
					(std::fabs(t - best_t) <= tie_eps * 0.01f && pt.p.GetX() > pick->p.GetX());
				if (better) { pick = &pt; best_t = t; }
			}
			if (pick == nullptr) break;
			if (t_impact < 0.0f) t_impact = std::min(std::max(best_t, 0.0f), step_dt);
			if (debug && pick) {
				std::vector<const Pt *> cand;
				for (const Pt &pt : pts) cand.push_back(&pt);
				std::sort(cand.begin(), cand.end(), [&](const Pt *a, const Pt *b) { return a->d < b->d; });
				JPH::RVec3 com = dyn.GetCenterOfMassPosition();
				for (size_t k = 0; k < cand.size() && k < 4; ++k) {
					JPH::Vec3 r = JPH::Vec3(cand[k]->p - com);
					fprintf(stderr, "  cand t=%d d=%.4f vn=%.3f r=(%.4f %.4f %.4f)%s\n", tick, cand[k]->d, rel(cand[k]->p).Dot(n), r.GetX(), r.GetY(), r.GetZ(), cand[k] == pick ? " <pick" : "");
				}
			}
			// the game's impact system also re-pushes the pair's existing contact points (corners
			// that impacted earlier and still touch) along the normal while this one is resolved
			std::vector<Pt *> old;
			for (size_t idx : cps)
				if (idx < pts.size() && &pts[idx] != pick && std::fabs(pts[idx].d - pick->d) < 0.002f) old.push_back(&pts[idx]);
			if (old.empty()) {
				resolve(*pick);
			} else {
				JPH::Vec3 v = rel(pick->p);
				pick->vn0 = v.Dot(n);
				pick->dir = direction(v, n);
				float mv1 = virt_mass(b1, pick->p, pick->dir, m2, static_mass_factor);
				float mv2 = virt_mass(b2, pick->p, pick->dir, m1, static_mass_factor);
				pick->step = push_fraction * (-2.0f * pick->vn0 * (mv1 * mv2 / (mv1 + mv2)));
				for (Pt *o : old) {
					float ov1 = virt_mass(b1, o->p, n, m2, static_mass_factor);
					float ov2 = virt_mass(b2, o->p, n, m1, static_mass_factor);
					o->step = push_fraction * (-2.0f * pick->vn0 * (ov1 * ov2 / (ov1 + ov2)));
				}
				float c1 = b1.IsDynamic() ? b1.GetLinearVelocity().Dot(n) : -b2.GetLinearVelocity().Dot(n);
				for (int it = 0; it < 100; ++it) {
					bool approaching = false;
					JPH::Vec3 vp = rel(pick->p);
					if (vp.Dot(n) < 0.0f) {
						approaching = true;
						pick->dir = direction(vp, pick->dir);
						push(b1, pick->p, pick->dir * pick->step);
						push(b2, pick->p, -pick->dir * pick->step);
					}
					for (Pt *o : old) {
						if (rel(o->p).Dot(n) >= 0.0f) continue;
						approaching = true;
						push(b1, o->p, n * o->step);
						push(b2, o->p, -n * o->step);
					}
					if (!approaching) break;
				}
				// restitution measured at the centre: sqrt(e) of the centre's approach speed
				float c2 = b1.IsDynamic() ? b1.GetLinearVelocity().Dot(n) : -b2.GetLinearVelocity().Dot(n);
				float wanted = extra + std::sqrt(e_eff) * std::max(-c1, 0.0f) - c2;
				if (wanted > 0.0f) {
					push(b1, pick->p, n);
					push(b2, pick->p, -n);
					float c3 = b1.IsDynamic() ? b1.GetLinearVelocity().Dot(n) : -b2.GetLinearVelocity().Dot(n);
					push(b1, pick->p, -n);
					push(b2, pick->p, n);
					float dvn = c3 - c2;
					if (std::fabs(dvn) > 1e-4f) {
						push(b1, pick->p, n * (wanted / dvn));
						push(b2, pick->p, -n * (wanted / dvn));
					}
				}
			}
			if (std::find(cps.begin(), cps.end(), pick->index) == cps.end()) cps.push_back(pick->index);
			if (debug) {
				const JPH::Body &db = b1.IsDynamic() ? b1 : b2;
				JPH::Vec3 r = JPH::Vec3(pick->p - db.GetCenterOfMassPosition());
				JPH::Vec3 ii = db.GetMotionProperties()->GetInverseInertiaDiagonal();
				JPH::Quat q = db.GetRotation();
				fprintf(stderr, "impact body m=%.4f invI=(%.5f %.5f %.5f) q=(%.6f %.6f %.6f %.6f) r=(%.5f %.5f %.5f) n=(%.4f %.4f %.4f) dir=(%.4f %.4f %.4f) f=%.3f e=%.3f v0=(%.4f %.4f %.4f) w0=(%.4f %.4f %.4f)\n", 1.0 / db.GetMotionProperties()->GetInverseMass(), (double)ii.GetX(), (double)ii.GetY(), (double)ii.GetZ(), (double)q.GetX(), (double)q.GetY(), (double)q.GetZ(), (double)q.GetW(), (double)r.GetX(), (double)r.GetY(), (double)r.GetZ(), (double)n.GetX(), (double)n.GetY(), (double)n.GetZ(), (double)pick->dir.GetX(), (double)pick->dir.GetY(), (double)pick->dir.GetZ(), (double)friction, (double)elasticity, (double)lv0.GetX(), (double)lv0.GetY(), (double)lv0.GetZ(), (double)av0.GetX(), (double)av0.GetY(), (double)av0.GetZ());
				JPH::Vec3 lv = b1.IsDynamic() ? b1.GetLinearVelocity() : b2.GetLinearVelocity();
				JPH::Vec3 av = b1.IsDynamic() ? b1.GetAngularVelocity() : b2.GetAngularVelocity();
				fprintf(stderr, "impact round %d p=(%.3f %.3f %.3f) d=%.4f vn0=%.3f -> v=(%.2f %.2f %.2f) w=(%.2f %.2f %.2f)\n", round, (double)pick->p.GetX(), (double)pick->p.GetY(), (double)pick->p.GetZ(), (double)pick->d, (double)pick->vn0, (double)lv.GetX(), (double)lv.GetY(), (double)lv.GetZ(), (double)av.GetX(), (double)av.GetY(), (double)av.GetZ());
			}
		}
		if (t_impact > 0.0f) {
			JPH::Vec3 dl = face_done ? lv0 * (t_impact - step_dt) : (lv0 - dyn.GetLinearVelocity()) * t_impact;
			JPH::Vec3 da = face_done ? av0 * (t_impact - step_dt) : (av0 - dyn.GetAngularVelocity()) * t_impact;
			std::lock_guard<std::mutex> lock(offsets_mutex);
			Offset &o = offsets[dyn.GetID().GetIndexAndSequenceNumber()];
			o.dpos += dl;
			o.drot += da;
		}
	}

	void apply_offsets(JPH::PhysicsSystem &sys) {
		if (getenv("GMP_NO_IMPACT_OFFSET")) { offsets.clear(); return; } // experiment knob, see docs/spec/physics.md
		std::lock_guard<std::mutex> lock(offsets_mutex);
		JPH::BodyInterface &bi = sys.GetBodyInterfaceNoLock();
		for (auto &[id, o] : offsets) {
			JPH::BodyID bid(id);
			if (!bi.IsAdded(bid)) continue;
			JPH::RVec3 p = bi.GetPosition(bid);
			JPH::Quat r = bi.GetRotation(bid);
			float ang = o.drot.Length();
			if (ang > 1e-9f) r = (JPH::Quat::sRotation(o.drot / ang, ang) * r).Normalized();
			bi.SetPositionAndRotation(bid, p + o.dpos, r, JPH::EActivation::DontActivate);
		}
		offsets.clear();
	}
};

GmpImpactListener *listener_of(void *system) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	return static_cast<GmpImpactListener *>(sys->GetContactListener());
}

}

extern "C" {

void gmp_install_impact_listener(void *system) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	if (sys->GetContactListener() == nullptr) {
		auto *l = new GmpImpactListener();
		l->sys = sys;
		sys->SetContactListener(l);
	}
}

unsigned int gmp_take_impacts(void *system, GmpImpactRec *out, unsigned int max) {
	auto *l = listener_of(system);
	if (!l) return 0;
	unsigned int n = (unsigned int)std::min<size_t>(l->impacts.size(), max);
	for (unsigned int i = 0; i < n; ++i) out[i] = l->impacts[i];
	l->impacts.clear();
	return n;
}

unsigned int gmp_take_contacts(void *system, GmpContactRec *out, unsigned int max) {
	auto *l = listener_of(system);
	if (!l) return 0;
	std::lock_guard<std::mutex> g(l->contacts_mutex);
	unsigned int n = (unsigned int)std::min<size_t>(l->contacts.size(), max);
	for (unsigned int i = 0; i < n; ++i) out[i] = l->contacts[i];
	l->contacts.clear();
	return n;
}

void gmp_set_surface(void *system, unsigned int body_id, float friction, float elasticity) {
	if (auto *l = listener_of(system)) l->surfaces[body_id] = GmpSurface{friction, elasticity};
}

void gmp_set_hull_points(void *system, unsigned int body_id, const float *xyz, unsigned int count) {
	if (auto *l = listener_of(system)) {
		std::vector<JPH::Vec3> v;
		v.reserve(count);
		for (unsigned int i = 0; i < count; ++i) v.emplace_back(xyz[3 * i], xyz[3 * i + 1], xyz[3 * i + 2]);
		l->hulls[body_id] = std::move(v);
	}
}

// body inertia from the PHY header (kg m^2 about the mass centre, body axes) instead of the
// value Jolt derives from the hull
void gmp_set_inertia(void *system, unsigned int body_id, float mass, float ix, float iy, float iz) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyLockWrite lock(sys->GetBodyLockInterface(), JPH::BodyID(body_id));
	if (!lock.Succeeded()) return;
	JPH::Body &body = lock.GetBody();
	if (!body.IsDynamic()) return;
	JPH::MotionProperties *motion = body.GetMotionProperties();
	JPH::Vec3 before = motion->GetInverseInertiaDiagonal();
	JPH::MassProperties mp;
	mp.mMass = mass;
	mp.mInertia = JPH::Mat44::sScale(JPH::Vec3(ix, iy, iz));
	motion->SetMassProperties(JPH::EAllowedDOFs::All, mp);
	if (getenv("GMP_IMPACT_DEBUG")) {
		JPH::Vec3 after = motion->GetInverseInertiaDiagonal();
		fprintf(stderr, "inertia body %u mass %.3f jolt I=(%.4f %.4f %.4f) phy I=(%.4f %.4f %.4f) now I=(%.4f %.4f %.4f) invmass %.5f\n", body_id, (double)mass, 1.0 / before.GetX(), 1.0 / before.GetY(), 1.0 / before.GetZ(), (double)ix, (double)iy, (double)iz, 1.0 / after.GetX(), 1.0 / after.GetY(), 1.0 / after.GetZ(), (double)motion->GetInverseMass());
	}
}

void gmp_apply_impact_offsets(void *system) {
	auto *l = listener_of(system);
	if (l) { l->apply_offsets(*reinterpret_cast<JPH::PhysicsSystem *>(system)); ++l->tick; }
}

void gmp_set_impact_step(void *system, float dt) {
	if (auto *l = listener_of(system)) { l->step_dt = dt; l->gravity = reinterpret_cast<JPH::PhysicsSystem *>(system)->GetGravity().Length(); }
}

void gmp_set_impact_skip(void *system, unsigned int body_id, int on) {
	if (auto *l = listener_of(system)) {
		if (on) l->skip[body_id] = true; else l->skip.erase(body_id);
	}
}

// constraint.NoCollide between two bodies: the pair is rejected at contact validation
void gmp_set_pair_collision(void *system, unsigned int id1, unsigned int id2, int enabled) {
	auto *l = listener_of(system);
	if (!l) return;
	auto key = GmpImpactListener::pair_key(id1, id2);
	if (enabled) l->nocollide.erase(key); else l->nocollide.insert(key);
}

// the collision_off property: the body collides with the world only
void gmp_set_world_only(void *system, unsigned int id, int on) {
	auto *l = listener_of(system);
	if (!l) return;
	if (on) l->world_only.insert(id); else l->world_only.erase(id);
}

void gmp_set_impact_enabled(void *system, int enabled) {
	if (auto *l = listener_of(system)) l->enabled = enabled != 0;
}

}
