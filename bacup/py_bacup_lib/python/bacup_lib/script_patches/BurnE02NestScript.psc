Function SpawnNestHazard(ObjectReference akNest)
	If HazardToSpawn == None || akNest == None || akNest.IsDestroyed()
		Return
	EndIf
	If HazardRefs == None
		HazardRefs = New ObjectReference[0]
	EndIf
	Int index = 0
	While index < HazardRefs.Length
		If HazardRefs[index] != None && HazardRefs[index].GetLinkedRef() == akNest
			Return
		EndIf
		index += 1
	EndWhile
	ObjectReference hazardRef = akNest.PlaceAtMe(HazardToSpawn)
	If hazardRef != None
		hazardRef.SetLinkedRef(akNest)
		HazardRefs.Add(hazardRef)
	EndIf
EndFunction

Function RemoveNestHazards(ObjectReference akNest)
	Int index = 0
	While HazardRefs != None && index < HazardRefs.Length
		ObjectReference hazardRef = HazardRefs[index]
		If hazardRef == None || akNest == None || hazardRef.GetLinkedRef() == akNest
			If hazardRef != None
				hazardRef.Delete()
			EndIf
			HazardRefs.Remove(index)
		Else
			index += 1
		EndIf
	EndWhile
EndFunction

Int Function CountIntactNests()
	Int intact = 0
	Int index = 0
	Int count = GetCount()
	While index < count
		ObjectReference nestRef = GetAt(index)
		If nestRef != None && !nestRef.IsDestroyed() && !nestRef.IsDisabled() && !nestRef.IsDeleted()
			intact += 1
		EndIf
		index += 1
	EndWhile
	Return intact
EndFunction

Function UpdateNestProgress()
	owningQuest = GetOwningQuest()
	If owningQuest == None || !owningQuest.IsRunning() || owningQuest.IsStageDone(9000) || owningQuest.IsStageDone(9991)
		Return
	EndIf

	; Stages 200/400/600 open a nest phase; 250/450/650 close it (DefaultCounterQuestA/B/C MyStage).
	Int phaseStage = -1
	DefaultCounterQuest counter = None
	String countName = ""
	String targetName = ""
	If owningQuest.IsStageDone(600) && !owningQuest.IsStageDone(650)
		phaseStage = 650
		counter = owningQuest as DefaultCounterQuestC
		countName = "nestsDestroyed3"
		targetName = "nestsDestroyedTarget3"
	ElseIf owningQuest.IsStageDone(400) && !owningQuest.IsStageDone(450)
		phaseStage = 450
		counter = owningQuest as DefaultCounterQuestB
		countName = "nestsDestroyed2"
		targetName = "nestsDestroyedTarget2"
	ElseIf owningQuest.IsStageDone(200) && !owningQuest.IsStageDone(250)
		phaseStage = 250
		counter = owningQuest as DefaultCounterQuestA
		countName = "nestsDestroyed"
		targetName = "nestsDestroyedTarget"
	Else
		Return
	EndIf

	Int intact = CountIntactNests()
	Int target = intact
	If counter != None
		target = counter.TargetValue
	EndIf
	Int destroyed = target - intact
	If destroyed < 0
		destroyed = 0
	EndIf
	B21:QuestVariables variables = owningQuest as B21:QuestVariables
	If variables != None
		variables.SetVariable(countName, destroyed as Float)
		variables.SetVariable(targetName, target as Float)
	EndIf

	If counter != None
		counter.Increment()
	EndIf
	; The phase also ends when every spawned nest is down, even if fewer spawned than the counter target.
	If intact <= 0 && !owningQuest.IsStageDone(phaseStage)
		owningQuest.SetStage(phaseStage)
	EndIf
EndFunction

Event OnLoad(ObjectReference akSenderRef)
	SpawnNestHazard(akSenderRef)
EndEvent

Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
	If akSenderRef == None || !akSenderRef.IsDestroyed()
		Return
	EndIf
	RemoveNestHazards(akSenderRef)
	UpdateNestProgress()
EndEvent

Event OnAliasShutdown()
	RemoveNestHazards(None)
EndEvent
