Event OnAliasInit()
	RegisterForHitEvent(Self)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, bool abPowerAttack, \
	Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	Actor targetActor = GetActorReference()
	Quest owningQuest = GetOwningQuest()
	If targetActor != None && akTarget == targetActor
		ApplyReachedStages(targetActor.GetValuePercentage(Game.GetHealthAV()))
	EndIf
	If owningQuest != None && owningQuest.IsRunning()
		RegisterForHitEvent(Self)
	EndIf
EndEvent

Event OnEnterBleedout()
	If GetActorReference() != None
		ApplyReachedStages(0.0)
	EndIf
EndEvent

Function ApplyReachedStages(Float targetHealthPercent)
	Quest owningQuest = GetOwningQuest()
	If owningQuest == None || !owningQuest.IsRunning() || StageData == None
		Return
	EndIf

	Int index = 0
	While index < StageData.Length
		StageDatum stageDatumToCheck = StageData[index]
		If stageDatumToCheck != None && stageDatumToCheck.StageToSet >= 0 && targetHealthPercent <= stageDatumToCheck.HealthPercent && !owningQuest.IsStageDone(stageDatumToCheck.StageToSet)
			owningQuest.SetStage(stageDatumToCheck.StageToSet)
		EndIf
		index += 1
	EndWhile
EndFunction
