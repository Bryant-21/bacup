Function RegisterGraftonEvents()
	If selfRef == None
		Return
	EndIf

	RegisterForAnimationEvent(selfRef, animEventOilBombSalvoFire1)
	RegisterForAnimationEvent(selfRef, animEventOilBombSalvoFire2)
	RegisterForAnimationEvent(selfRef, animEventOilBombSalvoFire3)
EndFunction

Function UnregisterGraftonEvents()
	If selfRef == None
		Return
	EndIf

	UnregisterForAnimationEvent(selfRef, animEventOilBombSalvoFire1)
	UnregisterForAnimationEvent(selfRef, animEventOilBombSalvoFire2)
	UnregisterForAnimationEvent(selfRef, animEventOilBombSalvoFire3)
EndFunction

Function ClearSalvoMarkers()
	If salvoSourceMarker != None
		salvoSourceMarker.Delete()
		salvoSourceMarker = None
	EndIf
	If salvoTargetMarker != None
		salvoTargetMarker.Delete()
		salvoTargetMarker = None
	EndIf
EndFunction

Function SetSalvoReady(Bool abReady)
	If selfRef == None || selfRef.IsDead()
		Return
	EndIf

	If AttackConditionAlt1 != None
		If abReady
			selfRef.SetValue(AttackConditionAlt1, 1.0)
		Else
			selfRef.SetValue(AttackConditionAlt1, 0.0)
		EndIf
	EndIf
	If abReady && OilBombSalvoWeapon != None
		selfRef.EquipItem(OilBombSalvoWeapon, False, True)
	ElseIf !abReady && OilBombWeapon != None
		selfRef.EquipItem(OilBombWeapon, False, True)
	EndIf
EndFunction

Function StartSalvoCooldown(Bool abInitial)
	Float minimumTime = SalvoCooldownTimeMin
	Float maximumTime = SalvoCooldownTimeMax
	If abInitial
		minimumTime = InitialSalvoAttackTimeMin
		maximumTime = InitialSalvoAttackTimeMax
	EndIf
	If maximumTime < minimumTime
		maximumTime = minimumTime
	EndIf

	SetSalvoReady(False)
	CancelTimer(oilBombSalvoCooldownTimerID)
	If maximumTime > 0.0
		StartTimer(Utility.RandomFloat(minimumTime, maximumTime), oilBombSalvoCooldownTimerID)
	Else
		SetSalvoReady(True)
	EndIf
EndFunction

Function LaunchOilBomb(String asSourceNode)
	If selfRef == None || selfRef.IsDead() || GraftonOilBombSalvoSpell == None || launchProjectileLock
		Return
	EndIf

	Actor targetActor = selfRef.GetCombatTarget()
	If targetActor == None || targetActor.IsDead()
		Return
	EndIf

	launchProjectileLock = True
	ClearSalvoMarkers()
	ObjectReference castSource = selfRef
	ObjectReference castTarget = targetActor
	If xMarker != None
		salvoSourceMarker = selfRef.PlaceAtNode(asSourceNode, xMarker)
		salvoTargetMarker = targetActor.PlaceAtMe(xMarker)
		If salvoSourceMarker != None
			castSource = salvoSourceMarker
		EndIf
		If salvoTargetMarker != None
			castTarget = salvoTargetMarker
		EndIf
	EndIf
	GraftonOilBombSalvoSpell.Cast(castSource, castTarget)
	launchProjectileLock = False
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akTarget
	If selfRef == None
		selfRef = akCaster
	EndIf
	If selfRef == None || selfRef.IsDead()
		Return
	EndIf

	RegisterGraftonEvents()
	StartSalvoCooldown(True)
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If selfRef == None || selfRef.IsDead() || akSource != selfRef
		Return
	EndIf

	If asEventName == animEventOilBombSalvoFire1
		oilBombSalvoSlot = 1
		LaunchOilBomb(oilBombSalvoLaunchNode1)
	ElseIf asEventName == animEventOilBombSalvoFire2
		oilBombSalvoSlot = 2
		LaunchOilBomb(oilBombSalvoLaunchNode2)
	ElseIf asEventName == animEventOilBombSalvoFire3
		oilBombSalvoSlot = 3
		LaunchOilBomb(oilBombSalvoLaunchNode3)
		StartSalvoCooldown(False)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == oilBombSalvoCooldownTimerID
		SetSalvoReady(True)
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(oilBombSalvoCooldownTimerID)
	UnregisterGraftonEvents()
	ClearSalvoMarkers()
	SetSalvoReady(True)
	selfRef = None
EndEvent
