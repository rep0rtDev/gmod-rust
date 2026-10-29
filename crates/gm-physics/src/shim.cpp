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
