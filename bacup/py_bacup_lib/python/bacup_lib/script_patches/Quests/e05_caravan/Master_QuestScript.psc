Event OnQuestInit()
	B21CaravanRunActive = False
	B21CaravanCooldownRemaining = 0.0
	B21CostaLastCompletedDay = 0
	RefreshCostaBusinessEligibility()
EndEvent

Function RefreshCostaBusinessEligibility()
	RefreshCaravanAvailability()
	RegisterForProtocolAdonais()

	ReferenceAlias vinnyAlias = GetAlias(2) as ReferenceAlias
	ObjectReference vinny = None
	If vinnyAlias != None
		vinny = vinnyAlias.GetReference()
	EndIf
	If vinny == None
		Return
	EndIf

	If GetRunningCostaBusinessQuest() != None || GetNextCostaBusinessStartKeyword() == None || !IsCostaBusinessDayAvailable()
		UnregisterForRemoteEvent(vinny, "OnActivate")
		Return
	EndIf

	RegisterForRemoteEvent(vinny, "OnActivate")
EndFunction

; --- Costa Business order and daily gate ---------------------------------------
; The seven Costa Business quests run in a fixed order, once per character, and at
; most one may be completed per game day. FO76 enforced that with each quest's own
; Story Manager condition block (GetEventData == its start keyword, GetGlobalValue
; MOON_LCP_Toggle_Dailies == 1, GetQuestCompleted(self) == 0, GetQuestCompleted(the
; previous quest) >= 1). The converted QUSTs carry no such block and every node under
; Moon_SQ06_Vera_Branch (6A9F35) is condition-free, so the gate lives here and each
; quest calls IsCostaBusinessQuestEligible from its own start stage.

Int Function GetCostaBusinessOrder(Quest akQuest)
	If akQuest == None
		Return 0
	EndIf
	Int questIndex = 1
	While questIndex <= 8
		If GetCostaBusinessQuest(questIndex) == akQuest
			Return questIndex
		EndIf
		questIndex += 1
	EndWhile
	Return 0
EndFunction

Bool Function IsCostaBusinessQuestEligible(Quest akQuest)
	Int order = GetCostaBusinessOrder(akQuest)
	If order <= 0 || order > 7 || akQuest.IsCompleted() || !AreCostaBusinessDailiesEnabled()
		Return False
	EndIf
	If !IsCostaBusinessDayAvailable()
		Return False
	EndIf

	Int questIndex = 1
	While questIndex < order
		Quest earlierQuest = GetCostaBusinessQuest(questIndex)
		If earlierQuest == None || !earlierQuest.IsCompleted()
			Return False
		EndIf
		questIndex += 1
	EndWhile
	Return True
EndFunction

Function NotifyCostaBusinessCompleted(Quest akQuest)
	If GetCostaBusinessOrder(akQuest) <= 0
		Return
	EndIf
	B21CostaLastCompletedDay = GetCurrentGameDay()
	RefreshCostaBusinessEligibility()
EndFunction

Bool Function IsCostaBusinessDayAvailable()
	Return B21CostaLastCompletedDay <= 0 || GetCurrentGameDay() > B21CostaLastCompletedDay
EndFunction

Int Function GetCurrentGameDay()
	Return (Utility.GetCurrentGameTime() as Int) + 1
EndFunction

; A missing toggle means the Skyline Valley content was not converted with its globals;
; treat that as "allowed" so the family is not bricked by an absent record.
Bool Function AreCostaBusinessDailiesEnabled()
	GlobalVariable dailiesToggle = Game.GetFormFromFile(0x006DAEE4, "SeventySix.esm") as GlobalVariable
	Return dailiesToggle == None || dailiesToggle.GetValue() >= 1.0
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

	; Talking to Vinny is also the moment his caravan line is evaluated, so re-check availability
	; and re-arm the poll here: it recovers the cycle even if the timer chain was lost.
	RefreshCaravanAvailability()

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

; --- Activity: Riding Shotgun (560B13) availability --------------------------------------------
; FO76's server decided when the caravan was ready to roll and reopened it on a cooldown. In FO4
; GV_IsRunning is the only gate the Vinny start line and both Story Manager nodes read, so this
; quest (StartsEnabled, never stops) owns it: 1 while the activity may be started, 0 while it runs
; and for Timer_CoolDown seconds afterwards.
Function RefreshCaravanAvailability()
	If GV_IsRunning == None
		Return
	EndIf

	Quest caravanQuest = Game.GetFormFromFile(0x00560B13, "SeventySix.esm") as Quest
	If caravanQuest == None
		Return
	EndIf

	If caravanQuest.IsRunning()
		B21CaravanRunActive = True
		B21CaravanCooldownRemaining = 0.0
		GV_IsRunning.SetValue(0.0)
	ElseIf B21CaravanRunActive
		B21CaravanRunActive = False
		B21CaravanCooldownRemaining = GetCaravanCooldownSeconds()
		GV_IsRunning.SetValue(0.0)
	ElseIf B21CaravanCooldownRemaining > 0.0
		GV_IsRunning.SetValue(0.0)
	Else
		GV_IsRunning.SetValue(1.0)
	EndIf

	StartTimer(GetCaravanPollSeconds(), 7805)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID != 7805
		Return
	EndIf

	If B21CaravanCooldownRemaining > 0.0
		B21CaravanCooldownRemaining -= GetCaravanPollSeconds()
		If B21CaravanCooldownRemaining <= 0.0
			B21CaravanCooldownRemaining = 0.0
			ResetCaravanForNextRun()
		EndIf
	EndIf

	RefreshCaravanAvailability()
EndEvent

; The activity keeps its stages, objectives and alias fills after it stops, so clear them once the
; cooldown is over: the next Story Manager start then behaves like a first run.
Function ResetCaravanForNextRun()
	Quest caravanQuest = Game.GetFormFromFile(0x00560B13, "SeventySix.esm") as Quest
	If caravanQuest != None && !caravanQuest.IsRunning()
		caravanQuest.Reset()
	EndIf
EndFunction

Float Function GetCaravanCooldownSeconds()
	If Timer_CoolDown == None
		Return 1200.0
	EndIf

	Float cooldown = Timer_CoolDown.GetValue()
	If cooldown <= 0.0
		Return 1200.0
	EndIf
	Return cooldown
EndFunction

Float Function GetCaravanPollSeconds()
	Return 5.0
EndFunction

Function TryStartEligibleCostaBusiness(ObjectReference akVinny, Actor akPlayer)
	If akVinny == None || akPlayer == None
		Return
	EndIf

	If GetRunningCostaBusinessQuest() != None || !IsCostaBusinessDayAvailable()
		UnregisterForRemoteEvent(akVinny, "OnActivate")
		Return
	EndIf

	Keyword startKeyword = GetNextCostaBusinessStartKeyword()
	Quest nextQuest = GetNextCostaBusinessQuest()
	If startKeyword == None || nextQuest == None
		UnregisterForRemoteEvent(akVinny, "OnActivate")
		Return
	EndIf

	; Every node under Moon_SQ06_Vera_Branch is condition-free, so SendStoryEventAndWait
	; reports True as soon as any sibling starts -- including one that then refuses itself
	; because it is out of order. Only the intended quest running counts as a start.
	startKeyword.SendStoryEventAndWait(akPlayer.GetCurrentLocation(), akPlayer)
	Bool started = nextQuest.IsRunning()
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

Quest Function GetNextCostaBusinessQuest()
	Int questIndex = 1
	While questIndex <= 7
		Quest costaQuest = GetCostaBusinessQuest(questIndex)
		If costaQuest == None || !costaQuest.IsCompleted()
			If costaQuest != None && !costaQuest.IsRunning()
				Return costaQuest
			EndIf
			Return None
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
