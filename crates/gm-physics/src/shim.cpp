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
