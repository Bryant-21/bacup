Function EndFX()
	If selfRef != None && SelfDestructingVFX != None
		SelfDestructingVFX.Stop(selfRef as ObjectReference)
	EndIf
	If selfDestructingSoundInstanceID != 0
		Sound.StopInstance(selfDestructingSoundInstanceID)
		selfDestructingSoundInstanceID = 0
	EndIf
EndFunction

Function StartFX()
	If selfRef != None && SelfDestructingVFX != None
		SelfDestructingVFX.Play(selfRef as ObjectReference, -1.0, None)
	EndIf
	If selfRef != None && SelfDestructingSound != None
		selfDestructingSoundInstanceID = SelfDestructingSound.Play(selfRef as ObjectReference)
	EndIf
EndFunction

Function ResetSelfDestruct()
	CancelTimer(selfDestructTimerID)
	EndFX()
	If selfRef != None
		If SpeedMult != None
			selfRef.SetValue(SpeedMult, speedMultOriginalValue)
		EndIf
		If SelfDestructingCloakSpell != None
			selfRef.RemoveSpell(SelfDestructingCloakSpell)
		EndIf
	EndIf
	GoToState("")
EndFunction

Function StartSelfDestruct()
	If selfRef == None || SelfDestructExplosion == None
		GoToState("")
		Return
	EndIf
	If NoSelfDestruct != None && selfRef.HasKeyword(NoSelfDestruct)
		GoToState("")
		Return
	EndIf
	If SpeedMult != None
		speedMultOriginalValue = selfRef.GetValue(SpeedMult)
		selfRef.SetValue(SpeedMult, SelfDestructingSpeed)
	EndIf
	If SelfDestructingWeapon != None
		selfRef.EquipItem(SelfDestructingWeapon)
	EndIf
	If SelfDestructingCloakSpell != None
		selfRef.AddSpell(SelfDestructingCloakSpell, False)
	EndIf
	StartFX()
	StartTimer(SelfDestructingTime, selfDestructTimerID)
EndFunction

Function ExplodeSelfDestruct()
	If selfRef == None || SelfDestructExplosion == None
		ResetSelfDestruct()
		Return
	EndIf
	If AvailableCondition1 != None
		selfRef.DamageValue(AvailableCondition1, 100.0)
	EndIf
	If AvailableCondition2 != None
		selfRef.DamageValue(AvailableCondition2, 100.0)
	EndIf
	If AvailableCondition3 != None
		selfRef.DamageValue(AvailableCondition3, 100.0)
	EndIf
	If AttackConditionAlt1 != None
		selfRef.DamageValue(AttackConditionAlt1, 100.0)
	EndIf
	If AttackConditionAlt2 != None
		selfRef.DamageValue(AttackConditionAlt2, 100.0)
	EndIf
	If AttackConditionAlt3 != None
		selfRef.DamageValue(AttackConditionAlt3, 100.0)
	EndIf
	If LeftAttackCondition != None
		selfRef.DamageValue(LeftAttackCondition, 100.0)
	EndIf
	If RightAttackCondition != None
		selfRef.DamageValue(RightAttackCondition, 100.0)
	EndIf
	selfRef.PlaceAtMe(SelfDestructExplosion)
	If NoDisintegrateOnSelfDestruct != None && selfRef.HasKeyword(NoDisintegrateOnSelfDestruct)
		If Health != None
			selfRef.DamageValue(Health, selfRef.GetValue(Health) + 100.0)
		EndIf
		ResetSelfDestruct()
	Else
		selfRef.Kill()
		If SelfDestructContainer != None
			selfRef.AttachAshPile(SelfDestructContainer)
		EndIf
		Utility.Wait(0.15)
		selfRef.SetAlpha(0.0, True)
	EndIf
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akTarget
	GoToState("SelfDestructing")
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	ResetSelfDestruct()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == selfDestructTimerID
		GoToState("selfdestructed")
	EndIf
EndEvent

State selfdestructed
	Event OnBeginState(String asOldState)
		EndFX()
		ExplodeSelfDestruct()
	EndEvent
EndState

State selfdestruct
	Event OnBeginState(String asOldState)
		GoToState("SelfDestructing")
	EndEvent
EndState

State SelfDestructing
	Event OnBeginState(String asOldState)
		StartSelfDestruct()
	EndEvent

	Event OnDying(Actor akKiller)
		CancelTimer(selfDestructTimerID)
		GoToState("selfdestructed")
	EndEvent
EndState
