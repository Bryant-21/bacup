Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfActorRef = akCaster
	If selfActorRef == None
		selfActorRef = akTarget
	EndIf
	If selfActorRef != None
		currentWeaponFireCount = 0
		RegisterForAnimationEvent(selfActorRef, animEventWeaponFire)
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	Actor liberator = akSource as Actor
	If liberator == None || liberator != selfActorRef || asEventName != animEventWeaponFire
		Return
	EndIf
	currentWeaponFireCount += 1
	If currentWeaponFireCount < WeaponFireMaxShots
		Return
	EndIf
	; The laser is an embedded weapon with no reload animation, so the rest
	; between magazines has to be driven from here: stow it to make the AI stop
	; firing, then hand it back when the timer expires.
	currentWeaponFireCount = 0
	selfActorRef.UnequipItem(LiberatorRangedWeapon, True, True)
	StartTimer(WeaponFireRestTime as Float, weaponFireTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == weaponFireTimerID && selfActorRef != None && !selfActorRef.IsDead()
		selfActorRef.EquipItem(LiberatorRangedWeapon, False, True)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(weaponFireTimerID)
	If selfActorRef != None
		UnregisterForAnimationEvent(selfActorRef, animEventWeaponFire)
	EndIf
EndEvent
