Event OnQuestInit()
	lock_StartRBQuest = False
	RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	lock_StartRBQuest = False
EndEvent

Bool Function BeginLocalBossEvent(Quest akEvent, ObjectReference akImpact)
	If akEvent == None || akImpact == None || lock_StartRBQuest || RBQuestList == None
		Return False
	EndIf

	Int index = 0
	While index < RBQuestList.Length
		RBQuestData entry = RBQuestList[index]
		If entry != None && entry.RBQuest == akEvent
			If entry.RBQuestStartKeyword == None || akEvent.IsRunning() || akEvent.IsStarting()
				Return False
			EndIf
			lock_StartRBQuest = True
			If !akEvent.IsStopped()
				akEvent.Stop()
			EndIf
			Int polls = 0
			While !akEvent.IsStopped() && polls < 50
				Utility.Wait(0.1)
				polls += 1
			EndWhile
			If !akEvent.IsStopped()
				lock_StartRBQuest = False
				Return False
			EndIf
			akEvent.Reset()
			Bool selected = entry.RBQuestStartKeyword.SendStoryEventAndWait(entry.RBTargetLoc, akImpact, Game.GetPlayer())
			polls = 0
			Int stable = 0
			While stable < 2 && (selected || akEvent.IsStarting()) && polls < 40
				Utility.Wait(0.25)
				polls += 1
				If akEvent.IsRunning()
					stable += 1
				Else
					stable = 0
				EndIf
			EndWhile
			lock_StartRBQuest = False
			Return stable >= 2
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction
