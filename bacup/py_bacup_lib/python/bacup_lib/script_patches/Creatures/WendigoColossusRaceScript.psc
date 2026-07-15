Function RegisterColossusEvents(Actor akColossus)
	If akColossus == None
		Return
	EndIf

	RegisterForAnimationEvent(akColossus, animEventPoisonVomitStart)
	RegisterForAnimationEvent(akColossus, animEventRadVomitStart)
EndFunction

Function UnregisterColossusEvents(Actor akColossus)
	If akColossus == None
		Return
	EndIf

	UnregisterForAnimationEvent(akColossus, animEventPoisonVomitStart)
	UnregisterForAnimationEvent(akColossus, animEventRadVomitStart)
EndFunction

Function StartCombatStyleTimer()
	Float maximumTime = CombatStyleSwitchTimeMax
	If maximumTime < CombatStyleSwitchTimeMin
		maximumTime = CombatStyleSwitchTimeMin
	EndIf
	If maximumTime > 0.0
		StartTimer(Utility.RandomFloat(CombatStyleSwitchTimeMin, maximumTime), combatStyleSwitchTimerID)
	EndIf
EndFunction

Function ApplyNextCombatStyle(Actor akColossus)
	If akColossus == None || akColossus.IsDead()
		Return
	EndIf

	If isNextStyleMelee
		If MeleeCombatStyle != None
			akColossus.SetCombatStyle(MeleeCombatStyle)
		EndIf
	ElseIf RangeCombatStyle != None
		akColossus.SetCombatStyle(RangeCombatStyle)
	EndIf
	isNextStyleMelee = !isNextStyleMelee
EndFunction

Function ClearVomitMarkers()
	If poisonVomitSourceMarker != None
		poisonVomitSourceMarker.Delete()
		poisonVomitSourceMarker = None
	EndIf
	If poisonVomitTargetMarker != None
		poisonVomitTargetMarker.Delete()
		poisonVomitTargetMarker = None
	EndIf
	If radVomitSourceMarker != None
		radVomitSourceMarker.Delete()
		radVomitSourceMarker = None
	EndIf
	If radVomitTargetMarker != None
		radVomitTargetMarker.Delete()
		radVomitTargetMarker = None
	EndIf
EndFunction

Function LaunchVomit(Actor akColossus, Spell akSpell, String asSourceNode, Bool abRadiation)
	If akColossus == None || akColossus.IsDead() || akSpell == None
		Return
	EndIf

	Actor targetActor = akColossus.GetCombatTarget()
	If targetActor == None || targetActor.IsDead()
		Return
	EndIf

	ObjectReference castSource = akColossus
	ObjectReference castTarget = targetActor
	If xMarker != None
		ObjectReference placedSource = akColossus.PlaceAtNode(asSourceNode, xMarker)
		ObjectReference placedTarget = targetActor.PlaceAtMe(xMarker)
		If placedSource != None
			castSource = placedSource
			If abRadiation
				radVomitSourceMarker = placedSource
			Else
				poisonVomitSourceMarker = placedSource
			EndIf
		EndIf
		If placedTarget != None
			castTarget = placedTarget
			If abRadiation
				radVomitTargetMarker = placedTarget
			Else
				poisonVomitTargetMarker = placedTarget
			EndIf
		EndIf
	EndIf
	akSpell.Cast(castSource, castTarget)
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	Actor colossus = akTarget
	If colossus == None
		colossus = akCaster
	EndIf
	If colossus == None || colossus.IsDead()
		Return
	EndIf

	RegisterColossusEvents(colossus)
	; Summon waves remain quest/server-owned; only locally bound combat mechanics are registered.
	isNextStyleMelee = False
	ApplyNextCombatStyle(colossus)
	StartCombatStyleTimer()
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	Actor colossus = GetTargetActor()
	If colossus == None || colossus.IsDead() || akSource != colossus
		Return
	EndIf

	ClearVomitMarkers()
	If asEventName == animEventPoisonVomitStart
		LaunchVomit(colossus, PoisonVomitLaunchSpell, nodePoisonVomitSourceName, False)
	ElseIf asEventName == animEventRadVomitStart
		LaunchVomit(colossus, RadVomitLaunchSpell, nodeRadVomitSourceName, True)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == combatStyleSwitchTimerID
		Actor colossus = GetTargetActor()
		ApplyNextCombatStyle(colossus)
		StartCombatStyleTimer()
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	CancelTimer(combatStyleSwitchTimerID)
	UnregisterColossusEvents(akTarget)
	ClearVomitMarkers()
	If akTarget != None
		akTarget.SetCombatStyle(None)
	EndIf
EndEvent
