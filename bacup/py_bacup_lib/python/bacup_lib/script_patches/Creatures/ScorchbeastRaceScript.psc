Function RegisterScorchbeastAnimationEvents()
	If selfRef == None
		Return
	EndIf

	RegisterForAnimationEvent(selfRef, animEventStartCloakVFX)
	RegisterForAnimationEvent(selfRef, animEventFlightLandingAttack)
	RegisterForAnimationEvent(selfRef, animEventFlightLanded)
	RegisterForAnimationEvent(selfRef, animEventGroundAreaAttack)
	RegisterForAnimationEvent(selfRef, animEventGroundTakeoffAttack)
	RegisterForAnimationEvent(selfRef, animEventGroundTakeoff)
	RegisterForAnimationEvent(selfRef, animEventSonicAttack)
EndFunction

Function UnregisterScorchbeastAnimationEvents()
	If selfRef == None
		Return
	EndIf

	UnregisterForAnimationEvent(selfRef, animEventStartCloakVFX)
	UnregisterForAnimationEvent(selfRef, animEventFlightLandingAttack)
	UnregisterForAnimationEvent(selfRef, animEventFlightLanded)
	UnregisterForAnimationEvent(selfRef, animEventGroundAreaAttack)
	UnregisterForAnimationEvent(selfRef, animEventGroundTakeoffAttack)
	UnregisterForAnimationEvent(selfRef, animEventGroundTakeoff)
	UnregisterForAnimationEvent(selfRef, animEventSonicAttack)
EndFunction

Function UpdateStrafeWeaponForState()
	If selfRef == None || selfRef.IsDead() || currentData == None
		Return
	EndIf

	If GetState() == "flying"
		If currentData.StrafeAttackWeapon != None
			selfRef.EquipItem(currentData.StrafeAttackWeapon, False, True)
		EndIf
	ElseIf currentData.StrafeAttackWeapon != None
		selfRef.UnequipItem(currentData.StrafeAttackWeapon, False, True)
	EndIf
EndFunction

Function RestoreSonicWeapon()
	If selfRef != None && !selfRef.IsDead() && currentData != None && currentData.SonicAttackWeapon != None
		selfRef.EquipItem(currentData.SonicAttackWeapon, False, True)
	EndIf
EndFunction

Function PlaceConfiguredExplosion(Explosion akExplosion)
	If selfRef != None && !selfRef.IsDead() && akExplosion != None
		selfRef.PlaceAtMe(akExplosion)
	EndIf
EndFunction

Function StartSonicAttackCooldown(Float afMinTime, Float afMaxTime)
	If selfRef == None || selfRef.IsDead() || currentData == None || currentData.SonicAttackWeapon == None
		Return
	EndIf

	Float cooldownMax = afMaxTime
	If cooldownMax < afMinTime
		cooldownMax = afMinTime
	EndIf

	; FO76 gates an equipped weapon slot with an API FO4 lacks, so toggle the configured weapon itself.
	selfRef.UnequipItem(currentData.SonicAttackWeapon, False, True)
	CancelTimer(sonicAttackCooldownTimerID)
	StartTimer(Utility.RandomFloat(afMinTime, cooldownMax), sonicAttackCooldownTimerID)
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akCaster
	If selfRef == None
		selfRef = akTarget
	EndIf
	If selfRef == None || selfRef.IsDead()
		Return
	EndIf

	If ActorTypeScorchbeastQueen != None && selfRef.HasKeyword(ActorTypeScorchbeastQueen)
		currentData = ScorchbeastQueenAttackData
	Else
		currentData = ScorchbeastAttackData
	EndIf
	If currentData == None
		Return
	EndIf

	RegisterScorchbeastAnimationEvents()
	GoToState("ground")
	RestoreSonicWeapon()
	UpdateStrafeWeaponForState()
	If ScorchBeastStartCloakVFXIdle != None
		selfRef.PlayIdle(ScorchBeastStartCloakVFXIdle)
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If selfRef == None || selfRef.IsDead() || akSource != selfRef || currentData == None
		Return
	EndIf

	If asEventName == animEventFlightLanded
		GoToState("ground")
		UpdateStrafeWeaponForState()
	ElseIf asEventName == animEventGroundTakeoff
		GoToState("flying")
		UpdateStrafeWeaponForState()
	ElseIf asEventName == animEventFlightLandingAttack
		PlaceConfiguredExplosion(currentData.LandingAttackExplosion)
	ElseIf asEventName == animEventGroundTakeoffAttack
		PlaceConfiguredExplosion(currentData.TakeOffAttackExplosion)
	ElseIf asEventName == animEventGroundAreaAttack
		PlaceConfiguredExplosion(currentData.AreaAttackExplosion)
	ElseIf asEventName == animEventSonicAttack
		If selfRef.IsInInterior()
			StartSonicAttackCooldown(currentData.InteriorSonicAttackCooldownTimeMin, currentData.InteriorSonicAttackCooldownTimeMax)
		ElseIf GetState() == "ground"
			StartSonicAttackCooldown(currentData.GroundSonicAttackCooldownTimeMin, currentData.GroundSonicAttackCooldownTimeMax)
		Else
			StartSonicAttackCooldown(currentData.FlyingSonicAttackCooldownTimeMin, currentData.FlyingSonicAttackCooldownTimeMax)
		EndIf
	ElseIf asEventName == animEventStartCloakVFX && ScorchBeastStartCloakVFXIdle != None
		selfRef.PlayIdle(ScorchBeastStartCloakVFXIdle)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == sonicAttackCooldownTimerID
		RestoreSonicWeapon()
		UpdateStrafeWeaponForState()
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(sonicAttackCooldownTimerID)
	UnregisterScorchbeastAnimationEvents()
	RestoreSonicWeapon()
	UpdateStrafeWeaponForState()
	selfRef = None
EndEvent
