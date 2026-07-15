Function RegisterBehemothBossEvents()
	If selfRef == None
		Return
	EndIf

	RegisterForAnimationEvent(selfRef, animEventSlam)
	RegisterForAnimationEvent(selfRef, animEventStomp)
	RegisterForAnimationEvent(selfRef, animEventSalvoFire)
EndFunction

Function UnregisterBehemothBossEvents()
	If selfRef == None
		Return
	EndIf

	UnregisterForAnimationEvent(selfRef, animEventSlam)
	UnregisterForAnimationEvent(selfRef, animEventStomp)
	UnregisterForAnimationEvent(selfRef, animEventSalvoFire)
EndFunction

Function ClearSalvoMarkers()
	If VisualTargetMarkerEffect != None && salvoTargetMarker != None
		VisualTargetMarkerEffect.Stop(salvoTargetMarker)
	EndIf
	If salvoSourceMarker != None
		salvoSourceMarker.Delete()
		salvoSourceMarker = None
	EndIf
	If salvoTargetMarker != None
		salvoTargetMarker.Delete()
		salvoTargetMarker = None
	EndIf
EndFunction

Function LaunchBossSalvo()
	If selfRef == None || selfRef.IsDead() || GraftonOilBombSalvoSpell == None
		Return
	EndIf
	If ChanceToTargetSalvo < 100.0 && Utility.RandomFloat(0.0, 100.0) > ChanceToTargetSalvo
		Return
	EndIf

	Actor targetActor = selfRef.GetCombatTarget()
	If targetActor == None || targetActor.IsDead()
		Return
	EndIf
	If CombatTargetRange > 0.0 && selfRef.GetDistance(targetActor) > CombatTargetRange
		Return
	EndIf

	ClearSalvoMarkers()
	ObjectReference castSource = selfRef
	ObjectReference castTarget = targetActor
	If xMarker != None
		salvoSourceMarker = selfRef.PlaceAtNode(salvoLaunchNode1, xMarker)
		If salvoSourceMarker != None
			castSource = salvoSourceMarker
		EndIf
	EndIf
	If VisualTargetMarker != None
		salvoTargetMarker = targetActor.PlaceAtMe(VisualTargetMarker)
		If salvoTargetMarker != None
			castTarget = salvoTargetMarker
		EndIf
	EndIf

	If VisualTargetMarkerEffect != None && salvoTargetMarker != None
		VisualTargetMarkerEffect.Play(salvoTargetMarker, SalvoLaunchDelay, selfRef)
	EndIf
	If SalvoLaunchDelay > 0.0
		Utility.Wait(SalvoLaunchDelay)
	EndIf
	If selfRef != None && !selfRef.IsDead()
		GraftonOilBombSalvoSpell.Cast(castSource, castTarget)
	EndIf
EndFunction

Event OnEffectStart(Actor akTarget, Actor akCaster)
	selfRef = akTarget
	If selfRef == None
		selfRef = akCaster
	EndIf
	If selfRef == None || selfRef.IsDead()
		Return
	EndIf

	RegisterBehemothBossEvents()
	If AttackConditionAlt1 != None
		selfRef.SetValue(AttackConditionAlt1, 1.0)
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If selfRef == None || selfRef.IsDead() || akSource != selfRef
		Return
	EndIf

	If asEventName == animEventSlam && SlamExplosion != None
		selfRef.PlaceAtMe(SlamExplosion)
	ElseIf asEventName == animEventStomp && StompExplosion != None
		selfRef.PlaceAtMe(StompExplosion)
	ElseIf asEventName == animEventSalvoFire
		If AttackConditionAlt1 != None
			selfRef.SetValue(AttackConditionAlt1, 0.0)
		EndIf
		LaunchBossSalvo()
		If selfRef != None && !selfRef.IsDead() && AttackConditionAlt1 != None
			selfRef.SetValue(AttackConditionAlt1, 1.0)
		EndIf
	EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
	UnregisterBehemothBossEvents()
	ClearSalvoMarkers()
	If selfRef != None && AttackConditionAlt1 != None
		selfRef.SetValue(AttackConditionAlt1, 1.0)
	EndIf
	selfRef = None
EndEvent
