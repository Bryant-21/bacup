Auto State Waiting
	Event OnCripple(ActorValue akActorValue, Bool abCrippled)
		If abCrippled
			Actor targetActor = GetTargetActor()
			If targetActor != None && targetActor.GetEquippedWeapon() == None && (!targetActor.HasKeyword(LeftHandWeaponKeyword) || targetActor.GetValue(LeftAttackCondition) <= 0.0) && (!targetActor.HasKeyword(RightHandWeaponKeyword) || targetActor.GetValue(RightAttackCondition) <= 0.0) && (!targetActor.HasKeyword(MiddleHandWeaponKeyword) || targetActor.GetValue(AttackConditionAlt1) <= 0.0)
				GoToState("selfdestruct")
			EndIf
		EndIf
	EndEvent

	Event OnActivate(ObjectReference akActionRef)
		If SelfDestructActivator != None && akActionRef == SelfDestructActivator
			GoToState("selfdestruct")
		EndIf
	EndEvent
EndState
