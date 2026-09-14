; Re-homed from OnSyncVariableNetworkChanged("RotationAngle"): FO76 replicated
; RotationAngle and clients ran the rotation. Single-player has no replication, so
; the setter is exposed locally and drives the same sound + keyframed rotation.
Function SetRotationAngle(Float afAngle)
	RotationAngle = afAngle
	utility.Wait(utility.RandomFloat(0.0, 1.5))
	Int instanceID = RotationSound.play(Self.GetLinkedRef(None))
	sound.SetInstanceVolume(instanceID, 1.0)
	Self.SetMotionType(Self.Motion_Keyframed, True)
	Self.SetAnimationVariableFloat("fspeed", 1.0)
	Self.SetAnimationVariableFloat("fvalue", RotationAngle)
	Self.playAnimation("play01")
EndFunction

; @drop-member OnSyncVariableNetworkChanged
