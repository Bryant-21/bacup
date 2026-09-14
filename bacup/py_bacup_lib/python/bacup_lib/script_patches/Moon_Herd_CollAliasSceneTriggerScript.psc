Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference collectionRef = GetAt(index)
		If collectionRef != None
			RegisterForRemoteEvent(collectionRef, "OnActivate")
			Actor collectionActor = collectionRef as Actor
			If collectionActor != None
				RegisterForRemoteEvent(collectionActor, "OnDying")
			EndIf
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		iActivationTriggers += 1
		EvaluateSceneTriggers(True, iActivationTriggers)
	EndIf
EndEvent

Event Actor.OnDying(Actor akSender, Actor akKiller)
	iDeathTriggers += 1
	EvaluateSceneTriggers(False, iDeathTriggers)
EndEvent

Function EvaluateSceneTriggers(Bool activationEvent, Int eventCount)
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && iTurnOffStage >= 0 && owningQuest.GetStage() >= iTurnOffStage
		Return
	EndIf
	Int index = 0
	While index < Triggers.Length
		SceneTrigger datum = Triggers[index]
		Bool eventMatches = (activationEvent && datum.triggerOnActivation) || (!activationEvent && datum.triggerOnDying)
		If eventMatches && !datum.hasBeenTriggered && eventCount >= datum.triggerCount && datum.sceneToTrigger != None
			If bForceSceneStart
				datum.sceneToTrigger.ForceStart()
			Else
				datum.sceneToTrigger.Start()
			EndIf
			datum.hasBeenTriggered = True
			Triggers[index] = datum
		EndIf
		index += 1
	EndWhile
EndFunction
