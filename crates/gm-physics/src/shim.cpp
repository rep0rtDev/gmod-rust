// Settings JoltC 0.3.1 does not expose. JPC_PhysicsSystem is a reinterpret_cast of
// JPH::PhysicsSystem (JoltC.cpp OPAQUE_WRAPPER), so the same cast works here.
#include <Jolt/Jolt.h>
#include <Jolt/Physics/PhysicsSystem.h>

extern "C" {

void gmp_set_contact_settings(void *system, float speculative_distance, float penetration_slop) {
	auto *sys = reinterpret_cast<JPH::PhysicsSystem *>(system);
	JPH::PhysicsSettings s = sys->GetPhysicsSettings();
	s.mSpeculativeContactDistance = speculative_distance;
	s.mPenetrationSlop = penetration_slop;
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
#include <Jolt/Physics/Constraints/FixedConstraint.h>
#include <Jolt/Physics/Constraints/DistanceConstraint.h>
#include <Jolt/Physics/Constraints/HingeConstraint.h>

extern "C" {

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
#include <vector>
#include <cmath>

namespace {

struct GmpSurface { float friction; float elasticity; };

class GmpImpactListener final : public JPH::ContactListener {
public:
	std::unordered_map<JPH::uint32, GmpSurface> surfaces;
	bool enabled = true;
	float static_mass_factor = 100000.0f;
	float step_dt = 0.015f;
	float persisted_threshold = 0.3f; // m/s, resting contacts stay with Jolt

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
		JPH::Vec3 n = -m.mWorldSpaceNormal; // pushes body 1 away from body 2
		float m1 = b1.IsDynamic() ? 1.0f / b1.GetMotionProperties()->GetInverseMass() : 0.0f;
		float m2 = b2.IsDynamic() ? 1.0f / b2.GetMotionProperties()->GetInverseMass() : 0.0f;
		if (m1 == 0.0f) m1 = static_mass_factor * m2;
		if (m2 == 0.0f) m2 = static_mass_factor * m1;
		// impact set: every manifold point that reaches the surface within this step (a flat
		// face lands on all its corners at once, an edge on one point)
		struct Pt { JPH::RVec3 p; float vn0; JPH::Vec3 d; float step; bool active; };
		std::vector<Pt> pts;
		for (JPH::uint i = 0; i < m.mRelativeContactPointsOn1.size(); ++i) {
			float pen = (m.mRelativeContactPointsOn1[i] - m.mRelativeContactPointsOn2[i]).Dot(m.mWorldSpaceNormal);
			JPH::RVec3 p = m.mBaseOffset + 0.5f * (m.mRelativeContactPointsOn1[i] + m.mRelativeContactPointsOn2[i]);
			JPH::Vec3 v = point_velocity(b1, p) - point_velocity(b2, p);
			float vn = v.Dot(n);
			if (vn >= -threshold) continue;
			if (pen < 0.0f && -pen > -vn * step_dt) continue; // speculative, does not touch this step
			pts.push_back(Pt{p, vn, n, 0.0f, true});
		}
		if (pts.empty()) return;
		// repeated impact counting (e_eff, 0.01 sqrt(n)) is left out: Jolt's resting contacts take over
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
		for (Pt &pt : pts) {
			pt.d = direction(rel(pt.p), n);
			float mv1 = virt_mass(b1, pt.p, pt.d, m2, static_mass_factor);
			float mv2 = virt_mass(b2, pt.p, pt.d, m1, static_mass_factor);
			pt.step = 0.1f * (-2.0f * pt.vn0 * (mv1 * mv2 / (mv1 + mv2)));
		}
		// push round robin until no point approaches any more
		for (int it = 0; it < 100; ++it) {
			bool any = false;
			for (Pt &pt : pts) {
				JPH::Vec3 v = rel(pt.p);
				float vn = v.Dot(n);
				if (vn >= 0.0f) continue;
				any = true;
				pt.d = direction(v, pt.d);
				push(b1, pt.p, pt.d * pt.step);
				push(b2, pt.p, -pt.d * pt.step);
			}
			if (!any) break;
		}
		// restitution: separation speed sqrt(e1 e2) |vn0| + 0.01 m/s at each point
		for (Pt &pt : pts) {
			float vn = rel(pt.p).Dot(n);
			float wanted = extra + std::sqrt(e_eff) * (-pt.vn0) - vn;
			if (wanted <= 0.0f) continue;
			push(b1, pt.p, pt.d);
			push(b2, pt.p, -pt.d);
			float dvn = rel(pt.p).Dot(n) - vn;
			push(b1, pt.p, -pt.d);
			push(b2, pt.p, pt.d);
			if (std::fabs(dvn) > 1e-4f) {
				float j = wanted / dvn;
				push(b1, pt.p, pt.d * j);
				push(b2, pt.p, -pt.d * j);
			}
		}
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

void gmp_set_impact_step(void *system, float dt) {
	if (auto *l = listener_of(system)) l->step_dt = dt;
}

void gmp_set_impact_enabled(void *system, int enabled) {
	if (auto *l = listener_of(system)) l->enabled = enabled != 0;
}

}
