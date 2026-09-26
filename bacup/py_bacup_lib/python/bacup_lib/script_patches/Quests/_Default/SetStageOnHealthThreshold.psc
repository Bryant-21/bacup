; Tales may set the same stages natively; every stage is guarded by IsStageDone so either side
; can arrive first.
Event OnAliasInit()
	Health = Game.GetHealthAV()
	CurrentIndex = 0
	Init = True
	RegisterForHitEvent(Self)
EndEvent

Event OnLoad()
	If !Init
		Health = Game.GetHealthAV()
		Init = True
	EndIf
	EvaluateThresholds()
	RegisterForHitEvent(Self)
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, \
	Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String asMaterialName)
	EvaluateThresholds()
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && owningQuest.IsRunning() && CurrentIndex < StageData.Length
		RegisterForHitEvent(Self)
	EndIf
EndEvent

Function EvaluateThresholds()
	Actor targetActor = GetActorReference()
	Quest owningQuest = GetOwningQuest()
	If targetActor == None || owningQuest == None || !owningQuest.IsRunning() || StageData == None
		Return
	EndIf
	If Health == None
		Health = Game.GetHealthAV()
	EndIf

	Float healthFraction = targetActor.GetValuePercentage(Health)
	If targetActor.IsDead()
		healthFraction = 0.0
	EndIf
	Int reached = 0
	Int index = 0
	While index < StageData.Length
		StageDatum datum = StageData[index]
		If datum != None && datum.StageToSet >= 0
			Float threshold = datum.HealthPercent
			If threshold > 1.0
				threshold = threshold / 100.0
			EndIf
			If healthFraction <= threshold && !owningQuest.IsStageDone(datum.StageToSet)
				owningQuest.SetStage(datum.StageToSet)
			EndIf
			If owningQuest.IsStageDone(datum.StageToSet)
				reached += 1
			EndIf
		Else
			reached += 1
		EndIf
		index += 1
	EndWhile
	CurrentIndex = reached
EndFunction
