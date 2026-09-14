Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
	RestartPointerTimer()
EndFunction

Function Fragment_Stage_0300_Item_00()
	TryStartSQ03()
	TryCompletePointer()
	RestartPointerTimer()
EndFunction

Function Fragment_Stage_0500_Item_00()
	TryStartSQ05()
	TryCompletePointer()
	RestartPointerTimer()
EndFunction

Function Fragment_Stage_9000_Item_00()
	CancelTimer(1)
	SetObjectiveCompleted(10)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID != 1 || !IsRunning() || IsStageDone(9000)
		Return
	EndIf

	If IsStageDone(300)
		TryStartSQ03()
	EndIf
	If IsStageDone(500)
		TryStartSQ05()
	EndIf
	TryCompletePointer()
	RestartPointerTimer()
EndEvent

Event OnQuestShutdown()
	CancelTimer(1)
EndEvent

Bool Function TryStartSQ03()
	Quest targetQuest = Game.GetFormFromFile(0x00729CBB, "SeventySix.esm") as Quest
	If QuestIsRunningOrComplete(targetQuest)
		Return True
	EndIf
	If targetQuest == None || AC_MQ02_Stage == None || !AC_MQ02_Stage.IsCompleted()
		Return False
	EndIf

	Keyword startKeyword = Game.GetFormFromFile(0x00729CC2, "SeventySix.esm") as Keyword
	Return SendPointerStoryEvent(startKeyword, 2)
EndFunction

Bool Function TryStartSQ05()
	Quest targetQuest = Game.GetFormFromFile(0x006FCF87, "SeventySix.esm") as Quest
	If QuestIsRunningOrComplete(targetQuest)
		Return True
	EndIf
	If targetQuest == None
		Return False
	EndIf

	Keyword startKeyword = Game.GetFormFromFile(0x006FCFC0, "SeventySix.esm") as Keyword
	Return SendPointerStoryEvent(startKeyword, 3)
EndFunction

Bool Function SendPointerStoryEvent(Keyword akStartKeyword, Int aiFlyerAliasID)
	ReferenceAlias flyerAlias = GetAlias(aiFlyerAliasID) as ReferenceAlias
	ObjectReference flyerRef
	If flyerAlias != None
		flyerRef = flyerAlias.GetReference()
	EndIf
	ObjectReference playerRef = Alias_Player.GetReference()
	If akStartKeyword == None || flyerRef == None || playerRef == None
		Return False
	EndIf

	Location eventLocation = flyerRef.GetCurrentLocation()
	If eventLocation == None
		eventLocation = playerRef.GetCurrentLocation()
	EndIf
	Return akStartKeyword.SendStoryEventAndWait(eventLocation, flyerRef, playerRef)
EndFunction

Bool Function QuestIsRunningOrComplete(Quest akQuest)
	Return akQuest != None && (akQuest.IsRunning() || akQuest.IsCompleted())
EndFunction

Function TryCompletePointer()
	If !IsStageDone(300) || !IsStageDone(500)
		Return
	EndIf

	Bool sq03Ready = TryStartSQ03()
	Bool sq05Ready = TryStartSQ05()
	If sq03Ready && sq05Ready
		SetStage(9000)
	EndIf
EndFunction

Function RestartPointerTimer()
	If !IsRunning() || IsStageDone(9000)
		Return
	EndIf
	CancelTimer(1)
	StartTimer(5.0, 1)
EndFunction
