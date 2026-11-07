// Settings JoltC 0.3.1 does not expose. JPC_PhysicsSystem is a reinterpret_cast of
// JPH::PhysicsSystem (JoltC.cpp OPAQUE_WRAPPER), so the same cast works here.
#include <mutex>
#include <Jolt/Jolt.h>
#include <Jolt/Physics/PhysicsSystem.h>
#include <Jolt/Physics/Collision/Shape/OffsetCenterOfMassShape.h>

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
#include <Jolt/Physics/Constraints/SixDOFConstraint.h>
#include <cstdlib>
#include <cfloat>
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

// weld: both bodies keep their current relative pose, softly: the game's weld gives at an
// impact (`constraint_weld_pair`: two welded crates landing at 260 u/s compress their 20 u
// offset to 10.65 u over four ticks and roll apart by 10 deg, then are pulled back over ~25
// ticks), so the six axes are springs: translation through the limit springs of a six DOF
// constraint, rotation through its position motors
static float weld_env(const char *name, float def) {
	const char *v = std::getenv(name);
	return v ? (float)std::atof(v) : def;
}
void *gmp_add_fixed_constraint(void *system, unsigned int id1, unsigned int id2) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID ids[2] = { JPH::BodyID(id1), JPH::BodyID(id2) };
	JPH::BodyLockMultiWrite lock(sys->GetBodyLockInterface(), ids, 2);
	JPH::Body *a = lock.GetBody(0), *b = lock.GetBody(1);
	if (!a || !b) return nullptr;
	// 60 Hz critically damped on all six axes: the landing pair rests within 1.0 / 1.2 u
	// (rigid: 2.8 / 3.0) and a hoverball welded to a crate still lifts it like the game
	// (`device_hoverball_crate` equal; 20 Hz tilts it 6 deg); the game's residual 1.8 u
	// deformation at rest and the lower crate's re-push at the landing tick (4.6 u peak,
	// our contact model, not the weld) remain open
	const float tf = weld_env("GMP_WELD_FREQ", 60.0f), td = weld_env("GMP_WELD_DAMP", 1.0f);
	const float rf = weld_env("GMP_WELD_ROT_FREQ", 15.0f), rd = weld_env("GMP_WELD_ROT_DAMP", 0.5f);
	// measured on `constraint_weld_pair` (+ its nocollide twin, identical through the
	// landing): the game's weld is a position correction, not a rigid joint. on the tick
	// after the lower crate lands it acts like a point impulse at a spot between the
	// hulls at the upper crate's height (upper +46 u/s y, -323 deg/s; lower -42 u/s y),
	// limited in force (the upper keeps falling at -206 of -268 u/s), and the
	// orientation is pulled back slowly over ~20 ticks while the pair slides. so: anchor
	// on the centre-of-mass line at the second body's height, translation = position
	// motors with a force limit per kilogram of the heavier body, rotation = 15 Hz
	// position motors at half damping (the pull-back of the tilted upper crate is what
	// drags the lower one +y 8 u over the next 20 ticks; 5 Hz gives 7 u short, 60 Hz
	// cancels the split within the solve and the pair slid 27 u). `device_hoverball_crate`
	// stays within tolerance.
	JPH::SixDOFConstraintSettings s;
	s.mSpace = JPH::EConstraintSpace::WorldSpace;
	{
		// anchor at the first body's centre of mass; the second's or the midpoint change
		// little (`constraint_weld_pair` 13 u either way, see docs/status.md)
		int anchor = (int)weld_env("GMP_WELD_ANCHOR", 3.0f);
		JPH::RVec3 pa = a->GetCenterOfMassPosition(), pb = b->GetCenterOfMassPosition();
		JPH::RVec3 horiz = pb + (pa - pb) * weld_env("GMP_WELD_ANCHOR_K", 0.5f);
		horiz.SetY(pb.GetY());
		s.mPosition1 = s.mPosition2 = anchor == 0 ? pa : anchor == 1 ? pb : anchor == 2 ? (pa + pb) * 0.5f : horiz;
	}
	using A = JPH::SixDOFConstraintSettings::EAxis;
	// the correction force is capped per kg of the heavier body; a weld to the world is uncapped
	const float ima = a->IsStatic() ? 0.0f : a->GetMotionProperties()->GetInverseMass();
	const float imb = b->IsStatic() ? 0.0f : b->GetMotionProperties()->GetInverseMass();
	const float im = std::min(ima, imb);
	const float force = im > 0.0f ? weld_env("GMP_WELD_FORCE", 80.0f) / im : FLT_MAX;
	for (int i = A::TranslationX; i <= A::TranslationZ; i++) {
		if (force > 0.0f) {
			s.MakeFreeAxis((A)i);
			s.mMotorSettings[i] = JPH::MotorSettings(tf, td, force, FLT_MAX);
		} else {
			s.SetLimitedAxis((A)i, -1e-4f, 1e-4f);
			s.mLimitsSpringSettings[i] = JPH::SpringSettings(JPH::ESpringMode::FrequencyAndDamping, tf, td);
		}
	}
	for (int i = A::RotationX; i <= A::RotationZ; i++) {
		s.MakeFreeAxis((A)i);
		s.mMotorSettings[i] = JPH::MotorSettings(rf, rd);
	}
	auto *c = static_cast<JPH::SixDOFConstraint *>(s.Create(*a, *b));
	for (int i = A::RotationX; i <= A::RotationZ; i++) c->SetMotorState((A)i, JPH::EMotorState::Position);
	if (force > 0.0f) {
		for (int i = A::TranslationX; i <= A::TranslationZ; i++) c->SetMotorState((A)i, JPH::EMotorState::Position);
		c->SetTargetPositionCS(JPH::Vec3::sZero());
	}
	// identity in constraint space keeps the pose the bodies were welded in; the body-space
	// variant would turn body 2 to body 1's rotation (a thruster can rolled 180 flew off)
	c->SetTargetOrientationCS(JPH::Quat::sIdentity());
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
		// the game catches a tick before the length is reached (balloon on a crate: taut at
		// 62.0 u + 112 u/s on a 64 u rope), so look two steps ahead
		bool taut = len + rate * dt * 2.0f > r.max_d;
		r.c->SetDistance(taut ? r.max_d : r.min_d, r.max_d);
		// the catch tick itself is a hard stop (171 deg/s on the hang against 120 through the spring)
		JPH::SpringSettings ss = ROPE_SPRING;
		if (taut && len < r.max_d) ss.mFrequency = 0.0f;
		r.c->SetLimitsSpringSettings(ss);
	}
}

// hinge (phys_hinge): world space point and axis in Jolt metres/axes
// ragdoll joint (a .phy `ragdollconstraint`): the child pinned to the parent at `p`, rotation
// about the child's frame axes limited per axis (radians, pyramid swing), with dry friction
void *gmp_add_ragdoll_constraint(void *system, unsigned int parent_id, unsigned int child_id, float px, float py,
                                 float pz, float qx, float qy, float qz, float qw, const float *limits,
                                 const float *friction) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID ids[2] = { JPH::BodyID(parent_id), JPH::BodyID(child_id) };
	JPH::BodyLockMultiWrite lock(sys->GetBodyLockInterface(), ids, 2);
	JPH::Body *a = lock.GetBody(0), *b = lock.GetBody(1);
	if (!a || !b) return nullptr;
	JPH::SixDOFConstraintSettings s;
	s.mSpace = JPH::EConstraintSpace::WorldSpace;
	s.mPosition1 = s.mPosition2 = JPH::RVec3(px, py, pz);
	JPH::Quat q = JPH::Quat(qx, qy, qz, qw).Normalized();
	s.mAxisX1 = s.mAxisX2 = q.RotateAxisX();
	s.mAxisY1 = s.mAxisY2 = q.RotateAxisY();
	s.mSwingType = JPH::ESwingType::Pyramid;
	using A = JPH::SixDOFConstraintSettings::EAxis;
	for (int i = 0; i < 3; ++i) s.MakeFixedAxis((A)i);
	for (int i = 0; i < 3; ++i) {
		float lo = limits[2 * i], hi = limits[2 * i + 1];
		if (hi <= lo) {
			s.MakeFixedAxis((A)(A::RotationX + i));
		} else {
			s.SetLimitedAxis((A)(A::RotationX + i), lo, hi);
		}
		s.mMaxFriction[A::RotationX + i] = friction[i];
	}
	JPH::Constraint *c = s.Create(*a, *b);
	sys->AddConstraint(c);
	return c;
}

void gmp_set_ragdoll_friction(void *constraint, float friction) {
	auto *c = reinterpret_cast<JPH::SixDOFConstraint *>(constraint);
	using A = JPH::SixDOFConstraintSettings::EAxis;
	for (int i = 0; i < 3; ++i) c->SetMaxFriction((A)(A::RotationX + i), friction);
}

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
	// Measured: a crate hanging on a ball socket loses energy at a constant 0.197/s whatever the
	// lever (24.5 / 35.7 / 63 u), speed (45..190 u/s) or amplitude, with air drag disabled;
	// a free crate, one on a rope or one on a hinge does not. Modelled as linear + angular
	// damping of that rate on the jointed bodies.
	const float bsd = weld_env("GMP_BS_DAMP", 0.197f);
	for (JPH::Body *bd : { a, b }) {
		if (bd->IsStatic()) continue;
		JPH::MotionProperties *mp = bd->GetMotionProperties();
		mp->SetLinearDamping(std::max(mp->GetLinearDamping(), bsd));
		mp->SetAngularDamping(std::max(mp->GetAngularDamping(), bsd));
	}
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

// `PhysObj:GetMass`: the body's mass (kg), 0 for a static/kinematic one
float gmp_body_mass(void *system, unsigned int id) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyLockRead lock(sys->GetBodyLockInterface(), JPH::BodyID(id));
	if (!lock.Succeeded()) return 0.0f;
	const JPH::Body &b = lock.GetBody();
	if (!b.IsDynamic()) return 0.0f;
	float inv = b.GetMotionProperties()->GetInverseMass();
	return inv > 0.0f ? 1.0f / inv : 0.0f;
}

// PhysObj:SetDamping / GetDamping: Jolt's per-body linear and angular damping
void gmp_set_damping(void *system, unsigned int id, float lin, float ang) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyLockWrite lock(sys->GetBodyLockInterface(), JPH::BodyID(id));
	if (!lock.Succeeded()) return;
	JPH::Body &b = lock.GetBody();
	if (!b.IsDynamic()) return;
	JPH::MotionProperties *mp = b.GetMotionProperties();
	mp->SetLinearDamping(lin < 0.0f ? 0.0f : lin);
	mp->SetAngularDamping(ang < 0.0f ? 0.0f : ang);
}

void gmp_body_damping(void *system, unsigned int id, float *out) {
	out[0] = 0.0f;
	out[1] = 0.0f;
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyLockRead lock(sys->GetBodyLockInterface(), JPH::BodyID(id));
	if (!lock.Succeeded()) return;
	const JPH::Body &b = lock.GetBody();
	if (!b.IsDynamic()) return;
	out[0] = b.GetMotionProperties()->GetLinearDamping();
	out[1] = b.GetMotionProperties()->GetAngularDamping();
}

// PhysObj:Wake
void gmp_activate(void *system, unsigned int id) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	sys->GetBodyInterface().ActivateBody(JPH::BodyID(id));
}

// world-space centre of mass of a body (the point a blast's impulse aims at)
void gmp_center_of_mass(void *system, unsigned int id, float *out) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyLockRead lock(sys->GetBodyLockInterface(), JPH::BodyID(id));
	if (!lock.Succeeded()) return;
	JPH::RVec3 c = lock.GetBody().GetCenterOfMassPosition();
	out[0] = float(c.GetX());
	out[1] = float(c.GetY());
	out[2] = float(c.GetZ());
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
	// convex pieces per body: the game's impact system works per ledge, so a body of several
	// pieces (the locker's feet) lands and rests piece by piece where one piece's face is a face
	std::unordered_map<JPH::uint32, unsigned int> hull_convexes;
	unsigned int convexes_of(const JPH::Body &b) const {
		auto it = hull_convexes.find(b.GetID().GetIndexAndSequenceNumber());
		return it == hull_convexes.end() ? 1u : it->second;
	}
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
	struct RestState { float vt_prev = 0.0f; float vn_in = 0.0f; int phase = 0; int landed_tick = -1; float land_vn = 0.0f; bool ring_land = false; };
	std::unordered_map<JPH::uint64, RestState> rest;
	// pair -> tick of its last strike above 0.8 m/s (the pivot rule's gate); unlike `rest` it
	// survives the sub-shape pairs of a multi-convex body coming and going every tick
	std::unordered_map<JPH::uint64, int> struck;
	// the game integrates up to the moment of impact with the old velocity and the rest of the
	// step with the new one; Jolt integrates the whole step with the new one, so after the step
	// every impacted body is moved by (v_old - v_new) * t_impact (and turned likewise)
	struct Offset { JPH::Vec3 dpos; JPH::Vec3 drot; JPH::Vec3 dpos_ccd; JPH::Vec3 drot_ccd; bool seen = false; JPH::RVec3 p0; JPH::Vec3 lv0; JPH::Vec3 lv1; float t_first = 0.0f; bool has_target = false; JPH::RVec3 target_pos; JPH::Quat target_rot; };
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
		// a body falling asleep loses its manifold too; its resting history stays, so a shot
		// that wakes it is a push from rest and not a landing of a body that never touched
		// (`shoot_prop_357_drum`: the asleep drum took rim vertex impacts and tumbled)
		if (sys) {
			const JPH::BodyInterface &bi = sys->GetBodyInterfaceNoLock();
			auto asleep = [&](JPH::BodyID id) {
				return bi.GetMotionType(id) != JPH::EMotionType::Static && !bi.IsActive(id);
			};
			if (asleep(pair.GetBody1ID()) || asleep(pair.GetBody2ID())) return;
		}
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
		// distinct points: 5 cm apart, or a quarter of the touching span for a small face (the
		// thruster can's bottom ring is 2.6 u across; merged into one corner, every tick of a
		// thruster pressing the can into the floor became a vertex impact and the can tumbled)
		float touch_span = 0.0f;
		for (const Pt &u : pts) for (const Pt &v : pts)
			if (u.d <= 0.001f && v.d <= 0.001f) touch_span = std::max(touch_span, (u.p - v.p).Length());
		const float distinct_min = std::min(0.05f, 0.25f * touch_span);
		for (const Pt &pt : pts) {
			if (pt.d > 0.001f) continue;
			bool distinct = true;
			for (const Pt *o : touch) if ((o->p - pt.p).Length() < distinct_min) { distinct = false; break; }
			if (distinct) touch.push_back(&pt);
		}
		int touching = (int)touch.size();
		// touching points along one line are an edge (or the rim arc of a tilted drum: three
		// neighbouring hull vertices 7 cm apart sag 0.5 cm), not a face: anchoring them held
		// the drum rocking back after its rim landing at a 15 deg tilt, where the game's falls
		// back flat (`device_dynamite_push`: z -143.6 at tick 79, ours -140.4)
		if (touching >= 3) {
			float best = 0.0f; const Pt *a = touch[0], *b = touch[0];
			for (const Pt *u : touch) for (const Pt *v : touch) {
				float l = (u->p - v->p).Length();
				if (l > best) { best = l; a = u; b = v; }
			}
			JPH::Vec3 dir = (b->p - a->p) / best;
			float off = 0.0f;
			for (const Pt *u : touch) {
				JPH::Vec3 r = u->p - a->p;
				off = std::max(off, (r - dir * r.Dot(dir)).Length());
			}
			if (off < 0.03f) touching = 2;
		}
		// touching points the centre of mass does not stand over are a pivot, not a resting
		// face: the locker landed on one row of feet rocks back under its weight in the game
		// (`phys_drop_flat_lockers_lo5`: -34 deg/s of pitch gone in 13 ticks, then back)
		// ... but only for a body that struck the other (a face or vertex impact) within the
		// last 2 s: a chair set down on its two feet (`server_chair_hull`, the game's stands
		// still) never struck
		JPH::uint64 pivot_key = (JPH::uint64(b1.GetID().GetIndexAndSequenceNumber()) << 32) | b2.GetID().GetIndexAndSequenceNumber();
		auto pivot_st = struck.find(pivot_key);
		bool recently_struck = pivot_st != struck.end() && tick - pivot_st->second <= 132;
		// the strike itself is seen here first, before the impact steps below record it: a
		// body arriving faster than 0.8 m/s (the locker dropped 2 u: 1.24; a body set on the
		// floor approaches at g dt = 0.23)
		float strike_vn = 0.0f;
		if (touching >= 3)
			for (const Pt *u : touch) strike_vn = std::min(strike_vn, (point_velocity(b1, u->p) - point_velocity(b2, u->p)).Dot(n));
		bool striking = strike_vn < -0.8f;
		if (touching >= 3 && (recently_struck || striking) && convexes_of(b1.IsDynamic() ? b1 : b2) > 1) {
			const JPH::Body &sb = b1.IsDynamic() ? b1 : b2;
			JPH::Vec3 e1 = n.GetNormalizedPerpendicular(), e2 = n.Cross(e1);
			JPH::Vec3 c3 = JPH::Vec3(sb.GetCenterOfMassPosition());
			std::vector<std::pair<float, float>> q;
			for (const Pt *u : touch) q.emplace_back(JPH::Vec3(u->p).Dot(e1), JPH::Vec3(u->p).Dot(e2));
			std::sort(q.begin(), q.end());
			auto cross = [](const std::pair<float, float> &o, const std::pair<float, float> &a, const std::pair<float, float> &b) {
				return (a.first - o.first) * (b.second - o.second) - (a.second - o.second) * (b.first - o.first);
			};
			std::vector<std::pair<float, float>> h(2 * q.size());
			size_t k = 0;
			for (size_t i2 = 0; i2 < q.size(); ++i2) {
				while (k >= 2 && cross(h[k - 2], h[k - 1], q[i2]) <= 0.0f) --k;
				h[k++] = q[i2];
			}
			for (size_t i2 = q.size() - 1, t2 = k + 1; i2 > 0; --i2) {
				while (k >= t2 && cross(h[k - 2], h[k - 1], q[i2 - 1]) <= 0.0f) --k;
				h[k++] = q[i2 - 1];
			}
			h.resize(k > 0 ? k - 1 : 0);
			std::pair<float, float> cp(c3.Dot(e1), c3.Dot(e2));
			bool inside = h.size() >= 3;
			for (size_t i2 = 0; inside && i2 < h.size(); ++i2) {
				const auto &a = h[i2], &b = h[(i2 + 1) % h.size()];
				float len = std::hypot(b.first - a.first, b.second - a.second);
				// 3 cm of margin: Jolt reports part of a lying body's face touching, the centre
				// of mass a hair outside it (the locker on its side, `phys_slip_lockers_v50`)
				if (len > 1e-6f && cross(a, b, cp) / len < -0.03f) inside = false;
			}
			if (!inside) {
				// the pivot row carries the body like a hinge: the inelastic normal impulse at the
				// row stops its fall (Jolt's soft contact let the 2000 kg locker sink 17 mm) and
				// its lever to the centre of mass is the restoring torque the game shows (the
				// locker's -34 deg/s of pitch gone in 13 ticks: m g d / I)
				JPH::Vec3 cen3 = JPH::Vec3::sZero();
				for (const Pt *u : touch) cen3 += JPH::Vec3(u->p);
				cen3 /= float(touch.size());
				JPH::Vec3 nd0 = b1.IsDynamic() ? n : -n;
				const JPH::Body &other0 = b1.IsDynamic() ? b2 : b1;
				float vn0 = (point_velocity(sb, cen3) - point_velocity(other0, cen3)).Dot(nd0);
				if (vn0 < 0.0f) {
					JPH::Vec3 r0 = cen3 - JPH::Vec3(sb.GetCenterOfMassPosition());
					JPH::Vec3 rn = r0.Cross(nd0);
					float k0 = sb.GetMotionProperties()->GetInverseMass() + rn.Dot(sb.GetInverseInertia().Multiply3x3(rn));
					if (k0 > 1e-9f) push(sb, cen3, nd0 * (-vn0 / k0));
				}
				touching = 2;
				if (striking) struck[pivot_key] = tick;
			}
			if (debug) fprintf(stderr, "support t=%d n_touch=%zu hull=%zu inside=%d strike_vn=%.3f recent=%d\n", tick, touch.size(), h.size(), (int)inside, strike_vn, (int)recently_struck);
		}
		// a landing still in progress (vertex impacts last step) whose touching points keep
		// arriving fast is the next impact, not a rest: the upright drum's -x rim a tick after
		// its +x rim (`device_dynamite_push`: +24.6 / +54.5 u/s and +107 deg/s in the game)
		bool arriving = false;
		{
			JPH::uint64 key0 = (JPH::uint64(b1.GetID().GetIndexAndSequenceNumber()) << 32) | b2.GetID().GetIndexAndSequenceNumber();
			auto it = rest.find(key0);
			// only a body still turning from its first impact (a shot resting drum keeps the
			// resting rules: `shoot_prop_*_drum`)
			const JPH::Body &dyn0 = b1.IsDynamic() ? b1 : b2;
			if (it != rest.end() && it->second.phase == 3 && it->second.landed_tick == tick - 1 && it->second.land_vn > 2.0f && dyn0.GetAngularVelocity().Length() > 0.5f)
				for (const Pt *t : touch) if ((point_velocity(b1, t->p) - point_velocity(b2, t->p)).Dot(n) < -0.3f) arriving = true;
			// a pair that has never landed, every touching point still descending faster than
			// the rest threshold, is arriving level, however close its points already are: the crate dropped
			// 2 u sits inside the speculative distance from its spawn, so its manifold is
			// persisted (not added) and its face reaches the plane with 0.8 mm to go at
			// 1.37 m/s - the game rebounds it 28 u/s, the resting rules parked it
			if ((it == rest.end() || it->second.landed_tick < 0) && !touch.empty()) {
				bool level = true;
				for (const Pt *t : touch) if ((point_velocity(b1, t->p) - point_velocity(b2, t->p)).Dot(n) >= -0.3f) level = false;
				if (level) arriving = true;
			}
		}
		// a round body rolling on the world: its touching points' tangential velocity is near
		// zero while its centre moves, and the hull's next vertex row is within 5 cm of the plane
		// (the drum's 16-gon: 2.7 cm; a crate pivoting on an edge has its next row 1 m up). The
		// game rolls it on at a steady rate (`phys_slide_barrel_v100`: 23 u/s at 95 deg/s over
		// 130 ticks); Jolt's contact on the flats and vertex rows braked ours to rest in 40 ticks
		// and the leading-edge anchor below in 25 - the floor carries a rolling body without
		// torque or friction
		if (!touch.empty() && !(b1.IsDynamic() && b2.IsDynamic())) {
			const JPH::Body &rb = b1.IsDynamic() ? b1 : b2;
			const JPH::Body &ob = b1.IsDynamic() ? b2 : b1;
			JPH::Vec3 rn = b1.IsDynamic() ? n : -n;
			JPH::Vec3 rv = rb.GetLinearVelocity() - ob.GetLinearVelocity();
			rv -= rn * rv.Dot(rn);
			float rvt = rv.Length();
			if (rvt > 0.25f && rb.GetAngularVelocity().Length() > 1.0f) {
				JPH::RVec3 cen = JPH::RVec3::sZero();
				for (const Pt *u : touch) cen += u->p;
				cen /= float(touch.size());
				JPH::Vec3 vc = point_velocity(rb, cen) - point_velocity(ob, cen);
				vc -= rn * vc.Dot(rn);
				float h = std::fabs(JPH::Vec3(rb.GetCenterOfMassPosition() - cen).Dot(rn));
				int next_row = 0;
				for (const Pt &pt : pts) if (pt.d > 0.001f && pt.d < 0.15f * h) ++next_row;
				
				// the contact line is the rotation axis laid in the plane (the touching points
				// themselves come one or two at a time on a vertex row)
				JPH::Vec3 line = rb.GetAngularVelocity();
				line -= rn * line.Dot(rn);
				bool on_line = line.Length() > 0.3f && std::fabs(line.Normalized().Dot(rb.GetAngularVelocity().Normalized())) > 0.9f;
				if (on_line) line = line.Normalized();
				// and it is the body's own axis: the principal axis of least inertia, well below the
				// next (the drum's 1.31 against 3.41; a crate's three are equal and a crate never rolls)
				float ax_dot = 0.0f, i_ratio = 0.0f;
				if (on_line) {
					const JPH::MotionProperties *rmp = rb.GetMotionProperties();
					JPH::Vec3 inv_d = rmp->GetInverseInertiaDiagonal();
					int imin = 0;
					for (int k = 1; k < 3; ++k) if (inv_d[k] > inv_d[imin]) imin = k;
					float i_min = 1.0f / std::max(inv_d[imin], 1e-9f), i_mid = 1e30f;
					for (int k = 0; k < 3; ++k) if (k != imin) i_mid = std::min(i_mid, 1.0f / std::max(inv_d[k], 1e-9f));
					JPH::Vec3 e = JPH::Vec3::sZero();
					e.SetComponent(imin, 1.0f);
					JPH::Vec3 axis = rb.GetRotation() * (rmp->GetInertiaRotation() * e);
					ax_dot = std::fabs(axis.Dot(line));
					i_ratio = i_min / i_mid;
					// a sphere (the hoverball: three equal inertias) rolls about any axis - the game's
					// ball dropped from 100 u rolls 100 u over 170 ticks at -676 -> -335 deg/s
					// (`constraint_device_hoverball_drop_m40`), ours stopped after 25
					float i_max = 0.0f;
					for (int k = 0; k < 3; ++k) i_max = std::max(i_max, 1.0f / std::max(inv_d[k], 1e-9f));
					bool sphere_like = i_max < 1.1f * i_min;
					if (!sphere_like && (i_min > 0.6f * i_mid || std::fabs(axis.Dot(line)) < 0.96f)) on_line = false;
				}
				// the centre of mass stands over the line (a rolling cylinder's does, within half a
				// facet; the upright drum tipping over its bottom rim has it 35 cm off the pivot)
				if (on_line) {
					JPH::Vec3 off = JPH::Vec3(rb.GetCenterOfMassPosition() - cen);
					off -= rn * off.Dot(rn);
					off -= line * off.Dot(line);
					if (off.Length() > 0.25f * h) on_line = false;
				}
				if (debug) fprintf(stderr, "rolltest t=%d touching=%d vt=%.3f vc=%.3f next_row=%d on_line=%d ax_dot=%.3f i_ratio=%.3f h=%.3f invd=(%.3f %.3f %.3f) line=(%.2f %.2f %.2f)\n", tick, touching, rvt, vc.Length(), next_row, (int)on_line, ax_dot, i_ratio, h, rb.GetMotionProperties()->GetInverseInertiaDiagonal().GetX(), rb.GetMotionProperties()->GetInverseInertiaDiagonal().GetY(), rb.GetMotionProperties()->GetInverseInertiaDiagonal().GetZ(), line.GetX(), line.GetY(), line.GetZ());
				if (on_line && vc.Length() < 0.35f * rvt && next_row >= 2) {
					if (b1.IsDynamic()) s.mInvInertiaScale1 = 0.0f; else s.mInvInertiaScale2 = 0.0f;
					JPH::uint64 rkey = (JPH::uint64(b1.GetID().GetIndexAndSequenceNumber()) << 32) | b2.GetID().GetIndexAndSequenceNumber();
					RestState &rrs = rest[rkey];
					rrs.phase = 0;
					rrs.vn_in = 0.0f;
					rrs.vt_prev = rvt;
					if (rrs.landed_tick < 0) rrs.landed_tick = tick;
					return;
				}
			}
		}
		if (touching >= 3 && !arriving) {
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
			// a body resting here (spawned on the floor) has landed as far as later arrivals are
			// concerned: only a pair that never touched takes the arriving-level rule above
			if (rs.landed_tick < 0) rs.landed_tick = tick;
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
			// the rule is the floor's: a prop sliding against another prop (two welded crates
			// 1 u apart, `constraint_weld_pair`) keeps full friction, the game's contact there
			// spins the upper crate to -323 deg/s on the tick after the landing
			bool pushed = !other.IsDynamic() && rs.phase != 3 && rs.vt_prev < 0.02f && vt > 0.1f;
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
				// the leading edge is every touching vertex level with the foremost one (a crate
				// sliding square has two front corners): anchoring one corner alone spun the
				// shot crate -50 deg/s and 17.6 u/s sideways (`shoot_prop_crowbar_crate`)
				// (box-like bodies only: a round can's tied rim vertices form an arc whose mean
				// sits inside the hull, `phys_drop_flat_paintcan_lo5`)
				Pt lead_mid;
				if (lead) {
					std::vector<const Pt *> tied;
					for (const Pt &pt : pts) {
						if (pt.d > 0.001f) continue;
						if (JPH::Vec3(pt.p - c).Dot(dir) >= best - 0.005f) tied.push_back(&pt);
					}
					int touching_now = 0;
					for (const Pt &pt : pts) if (pt.d <= 0.001f) ++touching_now;
					// a face resting on the floor (a can on its side touches along a line and
					// keeps its own vertex, `phys_drop_flat_paintcan_lo5`)
					if (touching_now >= 4 && tied.size() == 2 && (tied[0]->p - tied[1]->p).Length() > 0.2f) {
						lead_mid = *lead;
						lead_mid.p = 0.5f * (tied[0]->p + tied[1]->p);
						lead = &lead_mid;
					}
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
					// (a body pushed from rest the step before is still flat whatever spin the
					// push's torque left it: the crossbow bolt's drum turns 1.4 rad/s yet the game
					// takes its full 0.4236 v and lifts it 245 u/s, `shoot_prop_crossbow_drum`)
					bool flat = rs.phase == 1 || dyn.GetAngularVelocity().Length() < 0.4f;
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
		float approach0 = b1.IsDynamic() ? -lv0.Dot(n) : lv0.Dot(n); // the body's normal speed into the surface before this step's impacts
		float t_impact = -1.0f;
		bool face_done = false;
		float e_eff = elasticity;
		float extra = 0.0f;
		// the game's low-speed rebound is not restitution: the crate dropped flat leaves at
		// 28 u/s (0.72 m/s) from 45 to 170 u/s of approach whatever its mass (3 .. 2000 kg) and
		// whatever its elasticity (dirt 0.01 .. tile 0.3); only the pair friction moves it (ice
		// 0.1: 18 u/s, popcan 0.3: 19). Measured as a floor on a contact point's separation
		// speed, linear in the combined friction, for a face landing (three or more vertices
		// together); the rim vertex and edge landings keep plain restitution - the floor on
		// them launched rolling and sliding bodies (`phys_push_drum_v10..v40` and ten more
		// harnesses flipped); a point approaching slower than the face threshold is settling
		// (`phys_drop_flat_crate_*` 261103)
		// the floor's level is the body's: over seven boxes (crate 28 u/s, long crate 24.5,
		// cardboard boxes 22-23, hard drive 20.5, ammo can 17.8) it is linear in the lever
		// product b*h over the square radius of gyration about the body's first tangent axis
		// (b: half extent along the other tangent axis, h: mass centre height over the face),
		// with the friction on the slope; the crate's 0.417 + 0.467 f is this rule at b*h/k^2 =
		// 2.1 (`phys_drop_flat_{crate2,cardbox1,cardbox3,harddrive,ammocan}_lo*` 261103)
		float face_floor = 0.417f + 0.467f * friction;
		// the floor never sends a point back faster than it came (the series' slowest landing,
		// 45 u/s, still leaves at 0.62 of it): a body touching down slower than its floor is
		// settling and keeps plain restitution, or a chair spawned on the floor would hop
		// 0.8 u at its own floor speed forever (`server_duplicator_vehicle`)
		auto separation = [&](float restitution_speed, float approach) {
			if (approach <= 0.3f || approach <= face_floor) return restitution_speed;
			return std::max(restitution_speed, face_floor);
		};
		auto rel = [&](JPH::RVec3Arg p) { return point_velocity(b1, p) - point_velocity(b2, p); };
		// a speculative manifold whose closest pair is farther than the fastest vertex travels
		// this step is not touched this step: the model measures vertices against the manifold's
		// plane as if infinite, and a far surface's plane can pass through nearer geometry
		// (`use_chair`: gm_construct's displacement runs 4 u under the brush floor; its tilted
		// triangle's plane put a chair foot 4.3 cm away while the floor was at 1.9 cm, and the
		// chair was struck twice in one step - 1.7 u deep, skidding)
		{
			float sep = -m.mPenetrationDepth, vmax = 0.0f;
			for (const Pt &pt : pts) vmax = std::max(vmax, -rel(pt.p).Dot(n));
			if (sep > 0.0f && sep > vmax * step_dt + 1e-4f) return;
		}
		// does the vertex really reach the plane this step? Its velocity along the normal is a
		// linear extrapolation: a rim vertex of a wheel spinning over a gap approaches at rim
		// speed yet swings past (the game's wheel 2.3 u above the floor never touches, ours was
		// kicked at ~2000 deg/s); rotate the lever arm by w dt and look where it ends up
		auto reaches = [&](const Pt &pt) -> bool {
			const JPH::Body &dynb = b1.IsDynamic() ? b1 : b2;
			const JPH::Body &othb = b1.IsDynamic() ? b2 : b1;
			JPH::Vec3 w = dynb.GetAngularVelocity();
			float wl = w.Length();
			JPH::Vec3 r = JPH::Vec3(pt.p - dynb.GetCenterOfMassPosition());
			JPH::Vec3 r2 = wl > 1e-6f ? JPH::Quat::sRotation(w / wl, wl * step_dt) * r : r;
			JPH::Vec3 move = dynb.GetLinearVelocity() * step_dt + (r2 - r) - point_velocity(othb, pt.p) * step_dt;
			float dn = move.Dot(n) * (b1.IsDynamic() ? 1.0f : -1.0f);
			return pt.d + dn <= 0.0f;
		};
		// a manifold Jolt predicts speculatively (every point still off the plane) whose points
		// all swing past is no contact at all this step: without this Jolt's solver brakes the
		// spinning wheel over the gap (2741 -> 2220 deg/s in one tick) where the game's runs on
		{
			bool any_arrives = false;
			for (const Pt &pt : pts) if (pt.d <= 0.0f || reaches(pt)) { any_arrives = true; break; }
			if (!any_arrives) {
				s.mIsSensor = true;
				return;
			}
		}
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
		// the second corner of a fast landing (the step after the first strike, body still
		// turning from it) takes 0.6 of the friction tilt: the game's lockers at 400 u/s lose
		// 153 u/s of slide on it where the full tilt took 206 (`phys_slip_lockers_v400` 68 -> 38,
		// `phys_slip_filecab_v400` 35 -> 13.5, `phys_slip_lockers_v200` 27.5 -> 10.8, the drum's
		// second rim strike `phys_spin_barrel_x400` 61 -> 31; costs `phys_drop_tilted_filecab`
		// 8 -> 23 and `phys_slide_barrel_v400` 48 -> 59, sum over the 249 recordings 2056 -> 1959);
		// a ring landing's second step (the drum's antipode chain) keeps the full tilt - with
		// 0.6 `phys_slide_barrel` went 34 -> 152 and `phys_slide_barrel_v200` 7.7 -> 65
		bool second_step = false;
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
		bool flat_ring = false; // slow flat ring landing: normal push at the vertex, friction at the centre
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
			{ float tk = second_step ? tilt * 0.6f : tilt; c = std::cos(tk); sn = std::sin(tk); }
			pt.dir = direction(v, n);
			if (flat_ring) pt.dir = n;
			float jn_total = 0.0f;
			float mv1 = virt_mass(b1, pt.p, pt.dir, m2, static_mass_factor);
			float mv2 = virt_mass(b2, pt.p, pt.dir, m1, static_mass_factor);
			pt.step = push_fraction * (-2.0f * vn * (mv1 * mv2 / (mv1 + mv2)));
			// a point that arrives without slip (a prop falling straight onto a corner) sticks:
			// a step's tangential impulse is capped at what stops the tangential velocity the
			// normal part induces - the game's lockers (2000 kg) take 0.41 of the normal impulse
			// tangentially where the full Coulomb tilt gives 0.78; a corner that slides in keeps
			// the full tilt (the tilted crate's second corner)
			bool stick = !flat_ring; // measured on the slip series: the lockers stick at every entry slip
			for (int it = 0; it < 100 && vn < 0.0f; ++it) {
				push(b1, pt.p, pt.dir * pt.step);
				push(b2, pt.p, -pt.dir * pt.step);
				jn_total += pt.step;
				v = rel(pt.p);
				vn = v.Dot(n);
				pt.dir = direction(v, pt.dir);
				if (flat_ring) pt.dir = n;
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
			if (flat_ring) {
				const JPH::Body &dyn = b1.IsDynamic() ? b1 : b2;
				float mdyn = b1.IsDynamic() ? m1 : m2;
				JPH::Vec3 vc = dyn.GetLinearVelocity();
				JPH::Vec3 vt = vc - n * vc.Dot(n);
				float vt_len = vt.Length();
				if (vt_len > 1e-6f) {
					float jt = std::min(friction * jn_total, vt_len * mdyn);
					push(dyn, dyn.GetCenterOfMassPosition(), -(vt / vt_len) * jt);
				}
			}
			return true;
		};
		// one closest feature pair at a time, like the game: the vertex that touches first
		// (least distance, ties keep hull order), fully resolved, then the others are re-tested
		// with the new velocities; the game shows two impacts for a flat crate (a corner, then
		// the opposite one) and one rim vertex for the barrel
		// two vertices reaching the plane within 0.2 ms of each other are one arrival and the
		// hull order below picks between them: the drum tipping onto its side lands on a flat
		// whose two straddling rim vertices arrive 0.16 ms apart, and the game strikes the one
		// 0.3 mm nearer the plane, not the one 0.14 m/s faster (`phys_slide_barrel_v100`: its
		// axial spin -54 -> -122 deg/s, 73 -> 8.6 u; a 5 us window struck the other and killed
		// the spin; over the 249-recording sweep 2341 -> 2270, `phys_slip_lockers_v200` 21 -> 28)
		const float tie_eps = 0.02f;
		JPH::uint64 pair_key = (JPH::uint64(b1.GetID().GetIndexAndSequenceNumber()) << 32) | b2.GetID().GetIndexAndSequenceNumber();
		std::vector<size_t> &cps = contact_points[pair_key];
		// the second step of a landing takes the vertex chain (the antipode), not a face
		// a pair meeting for the first time with the face already through the plane is a body
		// spawned in the floor (the thruster can, 0.08 u deep): the game settles it upright
		bool fresh_pair = rest.find(pair_key) == rest.end();
		bool first_touch = fresh_pair || rest[pair_key].landed_tick < 0;
		// a landed body still turning is rocking: the next feature coming down is a pivot switch
		// (angular momentum about the new contact is kept, the old one lifts), not Jolt's face
		// contact that flattens it in a tick (`use_locker`: the locker on its two foot rows rocks
		// 5 degrees for 90+ ticks in the game, ours died on the second row's touchdown)
		bool rocking = rest.find(pair_key) != rest.end() && rest[pair_key].land_vn > 2.0f &&
			tick - rest[pair_key].landed_tick <= 60 &&
			(b1.IsDynamic() ? b1 : b2).GetAngularVelocity().Length() > 0.3f;
		bool landing_in_progress = (rest[pair_key].phase == 3 && rest[pair_key].landed_tick == tick - 1 && rest[pair_key].land_vn > 2.0f) || rocking;
		second_step = landing_in_progress && !rest[pair_key].ring_land;
		rest[pair_key].phase = 3; // landing in progress: the resting branch must not read it as a push
		if (verts_of_b2_used) cps.clear();
		for (int round = 0; round < 8; ++round) {
			// a face arriving flat (three or more vertices reach the plane this step, fastest
			// above 0.3 m/s) is one simultaneous impact, not a chain of vertex impacts: every
			// arriving vertex is driven to sqrt(e) of its own approach along its tilted push
			// direction (Gauss-Seidel, accumulated impulses never pull). The game's flat crate
			// at 653 u/s bounces 98 u/s with its yaw spin 148 -> 0 deg/s (`phys_spin_crate_z200`);
			// the vertex chain gave no bounce and a 211/169 deg/s tumble
			if (round == 0 && !landing_in_progress) {
				std::vector<Pt *> face;
				float fastest = 0.0f, t_first = step_dt;
				for (Pt &pt : pts) {
					float vn = rel(pt.p).Dot(n);
					if (vn >= -threshold || (pt.d > 0.0f && (pt.d > -vn * step_dt || !reaches(pt)))) continue;
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
				// a ring is round: its vertices sit at one radius from their centre (the drum's
				// 16-gon); the locker's ten feet put 40 vertices on the plane in a rectangle and
				// land as a face
				bool ring = face.size() > 8;
				if (ring) rest[pair_key].ring_land = true;
				if (ring) {
					JPH::Vec3 c = JPH::Vec3::sZero();
					for (const Pt *f : face) c += JPH::Vec3(f->p);
					c /= float(face.size());
					float rmin = 1e30f, rmax = 0.0f;
					for (const Pt *f : face) {
						float r = (JPH::Vec3(f->p) - c).Length();
						rmin = std::min(rmin, r);
						rmax = std::max(rmax, r);
					}
					ring = rmax < rmin * 1.15f;
				}
				// a ring arriving without spin is a single rim vertex impact in the game (the
				// upright drum of `device_dynamite_push` lands on its +x rim: -63 u/s of slip
				// stuck, -175 deg/s, the centre nearly stopped, the antipode a tick later); the
				// two-vertex rule below is the yaw-spinning drum's
				// many coplanar vertices that are not a ring are the feet of several convexes (the
				// locker: ten feet, 40 vertices) and do not land as a face in the game either: one
				// vertex is struck, the hull-local +x extreme nearest the x axis (`phys_drop_flat_lockers_*`:
				// from 72 u/s the game leaves at (-15.8, -1.2, 13.2) u/s hull-local with 34 deg/s
				// of pitch whatever the yaw - yaw90/yaw180 rotate the velocity with the hull - and
				// from 207 u/s at (-42, -3, 35) with 92 deg/s, enough to topple it)
				if (face.size() > 8 && !ring && convexes_of(b1.IsDynamic() ? b1 : b2) > 1) face.clear();
				if (ring) {
					const JPH::Body &dyn = b1.IsDynamic() ? b1 : b2;
					if (dyn.GetAngularVelocity().Length() < 0.5f) {
						face.clear();
						ring = false;
						JPH::Vec3 vc0 = dyn.GetLinearVelocity();
						float vt0 = (vc0 - n * vc0.Dot(n)).Length();
						// a flat ring landing that slides slowly (the drum kicked 50 u/s: 1.27 m/s) takes
						// its friction through the centre: the game's drum leaves the strike at 104 deg/s
						// and rocks back from -17 deg where the vertex friction with its 0.36 m lever gave
						// 120 deg/s about the slip axis and tipped it (`phys_slide_barrel_v50` 45 -> 6.2 u);
						// at 100 u/s and above the game tips exactly as the vertex friction does (v100 8.6,
						// v200 7.7, both 85+ u with centre friction), so the gate sits between the two
						flat_ring = vt0 > 0.3f && vt0 < 2.0f;
					}
				}
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
				bool spawned_in = first_touch && face.size() >= 3 && d_face <= 0.0f && face_slip < 3.0f;
				if (((face.size() >= 3 || edge_pair) && fastest > 0.3f && (d_face > 0.02f || (first_touch && d_face > 0.0f)) && face_slip < 3.0f) || spawned_in) {
					if (face.size() >= 3 && !ring) {
						const JPH::Body &fb = b1.IsDynamic() ? b1 : b2;
						const JPH::MotionProperties *fmp = fb.GetMotionProperties();
						JPH::Quat irot = fb.GetRotation().Conjugated();
						JPH::Vec3 nl = irot * n;
						int up = 0;
						for (int ax = 1; ax < 3; ++ax) if (std::fabs(nl[ax]) > std::fabs(nl[up])) up = ax;
						int rock = up == 0 ? 1 : 0;
						int lever = 3 - up - rock;
						float h = 0.0f, bext = 0.0f;
						for (Pt *f : face) {
							JPH::Vec3 r = irot * JPH::Vec3(f->p - fb.GetCenterOfMassPosition());
							h += std::fabs(r[up]);
							bext = std::max(bext, std::fabs(r[lever]));
						}
						h /= (float)face.size();
						JPH::Mat44 ii = fmp->GetLocalSpaceInverseInertia();
						float k2 = fmp->GetInverseMass() / std::max(ii(rock, rock), 1e-9f);
						face_floor = 0.36f + 0.264f * friction * (bext * h / k2);
					}
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
							float target = (edge_pair || ring) ? std::sqrt(e_eff) * (-pt.vn0) : separation(std::sqrt(e_eff) * (-pt.vn0), -pt.vn0);
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
					if (fastest > 0.8f) struck[pair_key] = tick; // a face impact is a strike (the pivot rule's gate)
					if (debug) fprintf(stderr, "strike t=%d face n=%zu fastest=%.3f spawned_in=%d first_touch=%d\n", tick, face.size(), fastest, (int)spawned_in, (int)first_touch);
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
			const JPH::Body &tie_body = b1.IsDynamic() ? b1 : b2;
			JPH::Quat tie_inv_rot = tie_body.GetRotation().Conjugated();
			JPH::RVec3 tie_com = tie_body.GetCenterOfMassPosition();
			for (Pt &pt : pts) {
				float vn = rel(pt.p).Dot(n);
				if (vn >= -threshold) continue;
				if (pt.d > 0.0f && (pt.d > -vn * step_dt || !reaches(pt))) continue;
				// a later vertex enters the step only if it reaches the plane in the time left
				// after the first touch (`phys_drop_edge_crate`: the far side needs 12 ms with 7 left)
				if (t_impact >= 0.0f && pt.d > 0.0f && pt.d > -vn * (step_dt - t_impact)) continue;
				float t = std::max(pt.d, 0.0f) / -vn;
				// ties: the game took the +x corner of the crate and the +x rim vertex of the barrel
				// vertices already through the plane all reach it at t = 0: the deepest is the
				// closest feature (the landed drum's -x rim, not a side vertex), +x only on equal depth
				// the +x is the hull's own (the yawed locker is struck on its local +x foot) and an
				// equal x (the locker's twenty +x foot vertices) takes the one nearest the x axis
				auto plus_x = [&](const Pt &q) {
					JPH::Vec3 rq = tie_inv_rot * JPH::Vec3(q.p - tie_com);
					JPH::Vec3 rp = tie_inv_rot * JPH::Vec3(pick->p - tie_com);
					if (rq.GetX() > rp.GetX() + 0.002f) return true;
					if (rq.GetX() < rp.GetX() - 0.002f) return false;
					return std::fabs(rq.GetZ()) < std::fabs(rp.GetZ()) - 1e-5f; // hull y is Jolt z
				};
				bool better = pick == nullptr || t < best_t - tie_eps * 0.01f ||
					(std::fabs(t - best_t) <= tie_eps * 0.01f &&
					 (landing_in_progress ? (pt.d < pick->d - 1e-5f || (std::fabs(pt.d - pick->d) <= 1e-5f && plus_x(pt)))
					                      : plus_x(pt)));
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
			rest[pair_key].landed_tick = tick; // a vertex impact is a landing step too
			rest[pair_key].land_vn = approach0;
			// a strike for the pivot rule: the weak re-impacts of a body settling on its feet
			// (0.13-0.46 m/s every tick, the placed chair's hardest 0.46) are not one; the locker
			// dropped 2 u arrives at 1.24
			if (approach0 > 0.8f) struck[pair_key] = tick;
			if (debug) fprintf(stderr, "strike t=%d vertex approach0=%.3f\n", tick, approach0);
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
			JPH::Vec3 lv1 = dyn.GetLinearVelocity(), av1 = dyn.GetAngularVelocity();
			// Jolt reaches this callback on two paths. Speculative contacts: the step is integrated
			// afterwards with the velocities set here, so the body overshoots by (v1 - v0) * t_impact.
			// Linear-cast CCD (a body crossing more than its inner radius): the step was already
			// integrated with the pre-impact velocities (position clamped to the time of impact,
			// rotation with av0) and the new velocities only act next step, so the post-impact motion
			// for the rest of the step is missing. Which path ran is read off the end pose in
			// apply_offsets (`use_chair`: a chair dropped flat lands through the cast; the
			// speculative-path offset alone tilted it 9 deg).
			JPH::Vec3 dl = face_done ? lv0 * (t_impact - step_dt) : (lv0 - lv1) * t_impact;
			JPH::Vec3 da = face_done ? av0 * (t_impact - step_dt) : (av0 - av1) * t_impact;
			std::lock_guard<std::mutex> lock(offsets_mutex);
			Offset &o = offsets[dyn.GetID().GetIndexAndSequenceNumber()];
			o.dpos += dl;
			o.drot += da;
			if (!face_done) {
				o.dpos_ccd += lv1 * (step_dt - t_impact);
				o.drot_ccd += (av1 - av0) * (step_dt - t_impact);
				if (!o.seen) { o.seen = true; o.p0 = dyn.GetPosition(); o.lv0 = lv0; o.t_first = t_impact; }
				o.lv1 = lv1;
			}
			if (face_done) {
				// Jolt integrates this step with whatever velocity the island solve leaves (the
				// bounce alone for a free crate, the weld's share for a welded one), so the
				// landing position is set absolutely: where the body was at the contact time
				// (`constraint_weld_pair`: the lower crate must end the step at the rest height
				// -123.7 like the game, not 2 u above it)
				o.has_target = true;
				o.target_pos = dyn.GetCenterOfMassPosition() - dyn.GetRotation() * dyn.GetShape()->GetCenterOfMass() + lv0 * t_impact;
				float a0 = av0.Length() * t_impact;
				o.target_rot = a0 > 1e-9f ? (JPH::Quat::sRotation(av0 / av0.Length(), a0) * dyn.GetRotation()).Normalized() : dyn.GetRotation();
			}
			if (debug) fprintf(stderr, "DBGOFF t=%d id=%u face_done=%d t_impact=%.5f dt=%.5f lv0=(%.3f %.3f %.3f) dl=(%.4f %.4f %.4f)\n", tick, dyn.GetID().GetIndex(), (int)face_done, (double)t_impact, (double)step_dt, (double)lv0.GetX(), (double)lv0.GetY(), (double)lv0.GetZ(), (double)dl.GetX(), (double)dl.GetY(), (double)dl.GetZ());
		}
	}

	void apply_offsets(JPH::PhysicsSystem &sys) {
		if (getenv("GMP_NO_IMPACT_OFFSET")) { offsets.clear(); return; } // experiment knob, see docs/spec/physics.md
		std::lock_guard<std::mutex> lock(offsets_mutex);
		JPH::BodyInterface &bi = sys.GetBodyInterfaceNoLock();
		for (auto &[id, o] : offsets) {
			JPH::BodyID bid(id);
			if (!bi.IsAdded(bid)) continue;
			if (o.has_target) {
				bi.SetPositionAndRotation(bid, o.target_pos, o.target_rot, JPH::EActivation::DontActivate);
				continue;
			}
			JPH::RVec3 p = bi.GetPosition(bid);
			JPH::Quat r = bi.GetRotation(bid);
			// speculative path: integrated past the callback with the new velocity; CCD path: left at
			// the time of impact with the old one (see the offsets' computation)
			bool ccd = o.seen && (p - (o.p0 + JPH::RVec3(o.lv0 * o.t_first))).LengthSq() < (p - (o.p0 + JPH::RVec3(o.lv1 * step_dt))).LengthSq();
			const JPH::Vec3 &dpos = ccd ? o.dpos_ccd : o.dpos;
			const JPH::Vec3 &drot = ccd ? o.drot_ccd : o.drot;
			if (getenv("GMP_IMPACT_DEBUG")) fprintf(stderr, "APPLYOFF id=%u ccd=%d\n", id, (int)ccd);
			float ang = drot.Length();
			if (ang > 1e-9f) r = (JPH::Quat::sRotation(drot / ang, ang) * r).Normalized();
			bi.SetPositionAndRotation(bid, p + dpos, r, JPH::EActivation::DontActivate);
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

void gmp_deactivate(void *system, unsigned int body_id) {
	reinterpret_cast<JPH::PhysicsSystem *>(system)->GetBodyInterface().DeactivateBody(JPH::BodyID(body_id));
}

void gmp_set_hull_convexes(void *system, unsigned int body_id, unsigned int count) {
	if (auto *l = listener_of(system)) l->hull_convexes[body_id] = count;
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

// A body hanging on a point / hinge joint from a non-moving body: Jolt integrates its position
// with the start-of-step tangential velocity and the next velocity solve projects away the radial
// part, losing (w*dt)^2 of the kinetic energy per step (0.03-0.06/s on a crate pendulum, where the
// game is lossless). Carry the velocity along with the body's rotation instead, like the lever it
// belongs to.
void gmp_carry_pivot_velocity(void *system, unsigned int id1, unsigned int id2, float dt) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::BodyID ids[2] = { JPH::BodyID(id1), JPH::BodyID(id2) };
	JPH::BodyLockMultiWrite lock(sys->GetBodyLockInterface(), ids, 2);
	JPH::Body *a = lock.GetBody(0), *b = lock.GetBody(1);
	if (!a || !b) return;
	bool da = a->GetMotionType() == JPH::EMotionType::Dynamic, db = b->GetMotionType() == JPH::EMotionType::Dynamic;
	if (da == db) return;
	JPH::Body *bd = da ? a : b;
	if (!bd->IsActive()) return;
	JPH::Vec3 w = bd->GetAngularVelocity();
	float wl = w.Length();
	if (wl < 1.0e-6f) return;
	JPH::Quat q = JPH::Quat::sRotation(w / wl, wl * dt);
	bd->GetMotionProperties()->SetLinearVelocityClamped(q * bd->GetLinearVelocity());
}

// Put the shape's centre of mass where the model's collision data says it is (body frame,
// metres): Jolt derives it from the hull volumes, the game reads it from the .phy.
void *gmp_offset_com_shape(void *shape, float cx, float cy, float cz) {
	auto *sh = reinterpret_cast<JPH::Shape *>(shape);
	JPH::Vec3 off = JPH::Vec3(cx, cy, cz) - sh->GetCenterOfMass();
	if (off.Length() < 1.0e-5f) return shape;
	JPH::OffsetCenterOfMassShapeSettings s(off, sh);
	JPH::Shape::ShapeResult r = s.Create();
	if (r.HasError()) return shape;
	JPH::Shape *out = r.Get().GetPtr();
	out->AddRef();
	return out;
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
