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
#include <Jolt/Physics/Constraints/HingeConstraint.h>
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
	JPH::Constraint *c = s.Create(*a, *b);
	sys->AddConstraint(c);
	return c;
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
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	sys->RemoveConstraint(reinterpret_cast<JPH::Constraint *>(constraint));
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

class GmpImpactListener final : public JPH::ContactListener {
public:
	std::unordered_map<JPH::uint32, GmpSurface> surfaces;
	std::unordered_map<JPH::uint32, std::vector<JPH::Vec3>> hulls;
	bool enabled = true;
	// body pairs that never collide (constraint.NoCollide); key is (min id << 32 | max id)
	std::unordered_set<unsigned long long> nocollide;
	static unsigned long long pair_key(JPH::uint32 a, JPH::uint32 b) {
		return ((unsigned long long)std::min(a, b) << 32) | std::max(a, b);
	}
	JPH::ValidateResult OnContactValidate(const JPH::Body &b1, const JPH::Body &b2, JPH::RVec3Arg,
	                                      const JPH::CollideShapeResult &) override {
		if (!nocollide.empty() && nocollide.count(pair_key(b1.GetID().GetIndexAndSequenceNumber(),
		                                                   b2.GetID().GetIndexAndSequenceNumber())))
			return JPH::ValidateResult::RejectAllContactsForThisBodyPair;
		return JPH::ValidateResult::AcceptAllContactsForThisBodyPair;
	}
	float static_mass_factor = 100000.0f;
	float step_dt = 0.015f;
	float persisted_threshold = 0.3f; // m/s, resting contacts stay with Jolt
	// size of one push as a fraction of the point's full stopping impulse; measured on the tilted
	// crate's corner impact: 0.03 to 0.075 reproduce the game's impulse within 0.5 %, 0.1 misses by 4.5 %
	// (docs/spec/physics.md); GMP_PUSH_FRACTION overrides for experiments
	float push_fraction = getenv("GMP_PUSH_FRACTION") ? (float)atof(getenv("GMP_PUSH_FRACTION")) : 0.075f;
	bool debug = getenv("GMP_IMPACT_DEBUG") != nullptr;
	std::unordered_map<JPH::uint64, std::vector<size_t>> contact_points; // pair -> hull vertex indices that already impacted
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
		contact_points.erase((JPH::uint64(pair.GetBody1ID().GetIndexAndSequenceNumber()) << 32) | pair.GetBody2ID().GetIndexAndSequenceNumber());
	}

	void OnContactPersisted(const JPH::Body &b1, const JPH::Body &b2, const JPH::ContactManifold &m, JPH::ContactSettings &s) override {
		impact(b1, b2, m, s, persisted_threshold);
	}

	void OnContactAdded(const JPH::Body &b1, const JPH::Body &b2, const JPH::ContactManifold &m, JPH::ContactSettings &s) override {
		impact(b1, b2, m, s, 1e-4f);
	}

	void impact(const JPH::Body &b1, const JPH::Body &b2, const JPH::ContactManifold &m, JPH::ContactSettings &s, float threshold) {
		GmpSurface s1 = surface(b1), s2 = surface(b2);
		float friction = std::min(1.0f, std::max(0.0f, s1.friction * s2.friction));
		float elasticity = std::min(1.0f, std::max(0.0f, s1.elasticity * s2.elasticity));
		s.mCombinedFriction = friction;
		s.mCombinedRestitution = 0.0f;
		if (!enabled || m.mRelativeContactPointsOn1.empty()) return;
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
		int touching = 0;
		for (const Pt &pt : pts) if (pt.d <= 0.001f) ++touching;
		if (touching >= 3) return;
		const JPH::Body &dyn = b1.IsDynamic() ? b1 : b2;
		JPH::Vec3 lv0 = dyn.GetLinearVelocity(), av0 = dyn.GetAngularVelocity();
		float t_impact = -1.0f;
		float e_eff = elasticity;
		float extra = 0.0f;
		auto rel = [&](JPH::RVec3Arg p) { return point_velocity(b1, p) - point_velocity(b2, p); };
		// friction tilt: tan(angle) = f (1 + sqrt(e)); push = c n - s (tangential part of the
		// unit relative velocity) unless the motion is already inside the friction cone
		float tilt = std::atan(friction * (1.0f + std::sqrt(elasticity)));
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
		auto resolve = [&](Pt &pt) {
			JPH::Vec3 v = rel(pt.p);
			float vn = v.Dot(n);
			if (vn >= 0.0f) return false;
			pt.vn0 = vn;
			pt.dir = direction(v, n);
			float mv1 = virt_mass(b1, pt.p, pt.dir, m2, static_mass_factor);
			float mv2 = virt_mass(b2, pt.p, pt.dir, m1, static_mass_factor);
			pt.step = push_fraction * (-2.0f * vn * (mv1 * mv2 / (mv1 + mv2)));
			for (int it = 0; it < 100 && vn < 0.0f; ++it) {
				push(b1, pt.p, pt.dir * pt.step);
				push(b2, pt.p, -pt.dir * pt.step);
				v = rel(pt.p);
				vn = v.Dot(n);
				pt.dir = direction(v, pt.dir);
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
		if (verts_of_b2_used) cps.clear();
		for (int round = 0; round < 8; ++round) {
			// the vertex that reaches the surface first: least time d / |vn| (the first round
			// has every vertex at the same distance, later rounds are decided by the rotation
			// the previous impact added); ties keep hull order
			Pt *pick = nullptr;
			float best_t = 0.0f;
			for (Pt &pt : pts) {
				float vn = rel(pt.p).Dot(n);
				if (vn >= -threshold) continue;
				if (pt.d > 0.0f && pt.d > -vn * step_dt) continue;
				float t = std::max(pt.d, 0.0f) / -vn;
				// ties: the game took the +x corner of the crate and the +x rim vertex of the barrel
				bool better = pick == nullptr || t < best_t - tie_eps * 0.01f ||
					(std::fabs(t - best_t) <= tie_eps * 0.01f && pt.p.GetX() > pick->p.GetX());
				if (better) { pick = &pt; best_t = t; }
			}
			if (pick == nullptr) break;
			if (t_impact < 0.0f) t_impact = std::min(std::max(best_t, 0.0f), step_dt);
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
			JPH::Vec3 dl = (lv0 - dyn.GetLinearVelocity()) * t_impact;
			JPH::Vec3 da = (av0 - dyn.GetAngularVelocity()) * t_impact;
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
	if (sys->GetContactListener() == nullptr) sys->SetContactListener(new GmpImpactListener());
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
	if (l) l->apply_offsets(*reinterpret_cast<JPH::PhysicsSystem *>(system));
}

void gmp_set_impact_step(void *system, float dt) {
	if (auto *l = listener_of(system)) l->step_dt = dt;
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

void gmp_set_impact_enabled(void *system, int enabled) {
	if (auto *l = listener_of(system)) l->enabled = enabled != 0;
}

}
