Event OnQuestInit()
	RefreshCostaBusinessEligibility()
EndEvent

Function RefreshCostaBusinessEligibility()
	RegisterForProtocolAdonais()

	ReferenceAlias vinnyAlias = GetAlias(2) as ReferenceAlias
	ObjectReference vinny = None
	If vinnyAlias != None
		vinny = vinnyAlias.GetReference()
	EndIf
	If vinny == None
		Return
	EndIf

	If GetRunningCostaBusinessQuest() != None || GetNextCostaBusinessStartKeyword() == None
		UnregisterForRemoteEvent(vinny, "OnActivate")
		Return
	EndIf

	RegisterForRemoteEvent(vinny, "OnActivate")
EndFunction

Function RegisterForProtocolAdonais()
	ReferenceAlias ariesAlias = GetAlias(6) as ReferenceAlias
	If ariesAlias != None && ariesAlias.GetReference() != None
		RegisterForRemoteEvent(ariesAlias.GetReference(), "OnActivate")
	EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActivator)
	Actor player = Game.GetPlayer()
	If player == None || akActivator != player
		Return
	EndIf

	ReferenceAlias vinnyAlias = GetAlias(2) as ReferenceAlias
	ReferenceAlias ariesAlias = GetAlias(6) as ReferenceAlias
	ObjectReference vinny = None
	ObjectReference aries = None
	If vinnyAlias != None
		vinny = vinnyAlias.GetReference()
	EndIf
	If ariesAlias != None
		aries = ariesAlias.GetReference()
	EndIf

	If akSender == vinny
		TryStartEligibleCostaBusiness(vinny, player)
	ElseIf akSender == aries
		TryStartProtocolAdonais(aries, player)
	EndIf
EndEvent

Function TryStartEligibleCostaBusiness(ObjectReference akVinny, Actor akPlayer)
	If akVinny == None || akPlayer == None
		Return
	EndIf

	If GetRunningCostaBusinessQuest() != None
		UnregisterForRemoteEvent(akVinny, "OnActivate")
		Return
	EndIf

	Keyword startKeyword = GetNextCostaBusinessStartKeyword()
	If startKeyword == None
		UnregisterForRemoteEvent(akVinny, "OnActivate")
		Return
	EndIf

	Bool started = startKeyword.SendStoryEventAndWait(akPlayer.GetCurrentLocation(), akPlayer)
	Keyword eugenieStartKeyword = Game.GetFormFromFile(0x006CC9A5, "SeventySix.esm") as Keyword
	If !started && startKeyword == eugenieStartKeyword
		Quest eugenieQuest = GetCostaBusinessQuest(2)
		If eugenieQuest != None && !eugenieQuest.IsCompleted() && !eugenieQuest.IsRunning()
			started = eugenieQuest.Start()
		EndIf
	EndIf

	If started
		UnregisterForRemoteEvent(akVinny, "OnActivate")
		WaitForCostaDialogueAndSetStage(akVinny as Actor, akPlayer, GetRunningCostaBusinessQuest())
	EndIf
EndFunction

Function TryStartProtocolAdonais(ObjectReference akAries, Actor akPlayer)
	If akAries == None || akPlayer == None
		Return
	EndIf

	Quest protocolQuest = GetCostaBusinessQuest(8)
	If protocolQuest == None || protocolQuest.IsCompleted() || protocolQuest.IsRunning()
		UnregisterForRemoteEvent(akAries, "OnActivate")
		Return
	EndIf

	If !AreCostaBusinessPrerequisitesComplete()
		Return
	EndIf

	Keyword startKeyword = Game.GetFormFromFile(0x006CAC84, "SeventySix.esm") as Keyword
	If startKeyword != None && startKeyword.SendStoryEventAndWait(akPlayer.GetCurrentLocation(), akPlayer)
		UnregisterForRemoteEvent(akAries, "OnActivate")
		WaitForCostaDialogueAndSetStage(akAries as Actor, akPlayer, protocolQuest)
	EndIf
EndFunction

Function WaitForCostaDialogueAndSetStage(Actor akSpeaker, Actor akPlayer, Quest akQuest)
	If akSpeaker == None || akPlayer == None || akQuest == None
		Return
	EndIf

	Int checks = 0
	While akSpeaker.GetDialogueTarget() != akPlayer && checks < 40
		Utility.Wait(0.25)
		checks += 1
	EndWhile
	If akSpeaker.GetDialogueTarget() != akPlayer
		Return
	EndIf

	checks = 0
	While akSpeaker.GetDialogueTarget() == akPlayer && checks < 2400
		Utility.Wait(0.25)
		checks += 1
	EndWhile
	If akSpeaker.GetDialogueTarget() != akPlayer && akQuest.IsRunning() && !akQuest.IsStageDone(200)
		akQuest.SetStage(200)
	EndIf
EndFunction

Quest Function GetRunningCostaBusinessQuest()
	Int questIndex = 1
	While questIndex <= 8
		Quest costaQuest = GetCostaBusinessQuest(questIndex)
		If costaQuest != None && costaQuest.IsRunning()
			Return costaQuest
		EndIf
		questIndex += 1
	EndWhile

	Return None
EndFunction

Keyword Function GetNextCostaBusinessStartKeyword()
	Quest kieranQuest = GetCostaBusinessQuest(1)
	If kieranQuest == None || !kieranQuest.IsCompleted()
		If kieranQuest != None && !kieranQuest.IsRunning()
			Return Game.GetFormFromFile(0x006CAC85, "SeventySix.esm") as Keyword
		EndIf
		Return None
	EndIf

	Quest costaQuest = GetCostaBusinessQuest(2)
	If costaQuest == None || !costaQuest.IsCompleted()
		If costaQuest != None && !costaQuest.IsRunning()
			Return Game.GetFormFromFile(0x006CC9A5, "SeventySix.esm") as Keyword
		EndIf
		Return None
	EndIf

	costaQuest = GetCostaBusinessQuest(3)
	If costaQuest == None || !costaQuest.IsCompleted()
		If costaQuest != None && !costaQuest.IsRunning()
			Return Game.GetFormFromFile(0x006CADEE, "SeventySix.esm") as Keyword
		EndIf
		Return None
	EndIf

	costaQuest = GetCostaBusinessQuest(4)
	If costaQuest == None || !costaQuest.IsCompleted()
		If costaQuest != None && !costaQuest.IsRunning()
			Return Game.GetFormFromFile(0x006CADC8, "SeventySix.esm") as Keyword
		EndIf
		Return None
	EndIf

	costaQuest = GetCostaBusinessQuest(5)
	If costaQuest == None || !costaQuest.IsCompleted()
		If costaQuest != None && !costaQuest.IsRunning()
			Return Game.GetFormFromFile(0x006CCAF3, "SeventySix.esm") as Keyword
		EndIf
		Return None
	EndIf

	costaQuest = GetCostaBusinessQuest(6)
	If costaQuest == None || !costaQuest.IsCompleted()
		If costaQuest != None && !costaQuest.IsRunning()
			Return Game.GetFormFromFile(0x006A9EEB, "SeventySix.esm") as Keyword
		EndIf
		Return None
	EndIf

	costaQuest = GetCostaBusinessQuest(7)
	If costaQuest == None || !costaQuest.IsCompleted()
		If costaQuest != None && !costaQuest.IsRunning()
			Return Game.GetFormFromFile(0x006CAC86, "SeventySix.esm") as Keyword
		EndIf
	EndIf

	Return None
EndFunction

Bool Function AreCostaBusinessPrerequisitesComplete()
	Int questIndex = 1
	While questIndex <= 7
		Quest costaQuest = GetCostaBusinessQuest(questIndex)
		If costaQuest == None || !costaQuest.IsCompleted()
			Return False
		EndIf
		questIndex += 1
	EndWhile

	Quest wolfQuest = Game.GetFormFromFile(0x003FBF2E, "SeventySix.esm") as Quest
	Quest outOfTheBlueQuest = Game.GetFormFromFile(0x005F5E1A, "SeventySix.esm") as Quest
	Return wolfQuest != None && wolfQuest.IsCompleted() && outOfTheBlueQuest != None && outOfTheBlueQuest.IsCompleted()
EndFunction

Quest Function GetCostaBusinessQuest(Int aiQuestIndex)
	If aiQuestIndex == 1
		Return Game.GetFormFromFile(0x0068FD4A, "SeventySix.esm") as Quest
	ElseIf aiQuestIndex == 2
		Return Game.GetFormFromFile(0x006A173A, "SeventySix.esm") as Quest
	ElseIf aiQuestIndex == 3
		Return Game.GetFormFromFile(0x006A0F94, "SeventySix.esm") as Quest
	ElseIf aiQuestIndex == 4
		Return Game.GetFormFromFile(0x006A1030, "SeventySix.esm") as Quest
	ElseIf aiQuestIndex == 5
		Return Game.GetFormFromFile(0x006A17A9, "SeventySix.esm") as Quest
	ElseIf aiQuestIndex == 6
		Return Game.GetFormFromFile(0x0069F2C7, "SeventySix.esm") as Quest
	ElseIf aiQuestIndex == 7
		Return Game.GetFormFromFile(0x006A21E3, "SeventySix.esm") as Quest
	ElseIf aiQuestIndex == 8
		Return Game.GetFormFromFile(0x006A21E4, "SeventySix.esm") as Quest
	EndIf

	Return None
EndFunction
