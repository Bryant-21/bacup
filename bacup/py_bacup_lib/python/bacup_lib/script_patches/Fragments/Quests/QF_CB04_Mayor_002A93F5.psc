CB04_QuestScript Function GetMayorQuestScript()
	Return Game.GetFormFromFile(0x002A93F5, "SeventySix.esm") as CB04_QuestScript
EndFunction

Function RegisterForCB04Trigger(ObjectReference akTriggerRef)
	If akTriggerRef != None
		RegisterForRemoteEvent(akTriggerRef, "OnTriggerEnter")
	EndIf
EndFunction

Function RegisterForWorkTerminal()
	ObjectReference workTerminalRef = Game.GetFormFromFile(0x00254689, "SeventySix.esm") as ObjectReference
	If workTerminalRef != None
		Terminal workTerminal = workTerminalRef.GetBaseObject() as Terminal
		If workTerminal != None
			RegisterForRemoteEvent(workTerminal, "OnMenuItemRun")
		EndIf
	EndIf
EndFunction

Function CountCarriedClues()
	Actor playerRef = Game.GetPlayer()
	CB04_QuestScript mayorQuest = GetMayorQuestScript()
	If playerRef == None || mayorQuest == None
		Return
	EndIf

	If playerRef.GetItemCount(CB04_HolotapeClue1) > 0
		mayorQuest.AddClue(1)
	EndIf
	If playerRef.GetItemCount(CB04_HolotapeClue2) > 0
		mayorQuest.AddClue(2)
	EndIf
	If playerRef.GetItemCount(CB04_Password_Note) > 0
		mayorQuest.AddClue(4)
	EndIf
	If playerRef.GetItemCount(CB04_Keycard) > 0
		mayorQuest.AddClue(5)
	EndIf
EndFunction

Function ClearClueInventoryListener()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnItemAdded")
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	RemoveAllInventoryEventFilters()
EndFunction

Function ReconcileClueInventoryListener()
	ClearClueInventoryListener()
	If !IsRunning() || IsCompleted() || GetCurrentStageID() >= 500
		Return
	EndIf

	If CB04_HolotapeClue1 != None
		AddInventoryEventFilter(CB04_HolotapeClue1)
	EndIf
	If CB04_HolotapeClue2 != None
		AddInventoryEventFilter(CB04_HolotapeClue2)
	EndIf
	If CB04_Password_Note != None
		AddInventoryEventFilter(CB04_Password_Note)
	EndIf
	If CB04_Keycard != None
		AddInventoryEventFilter(CB04_Keycard)
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnItemAdded")
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
EndFunction

Event OnQuestInit()
	ReconcileClueInventoryListener()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer()
		ReconcileClueInventoryListener()
	EndIf
EndEvent

Event OnQuestShutdown()
	ClearClueInventoryListener()
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf

	Int currentStage = GetCurrentStageID()
	ObjectReference hideoutTrigger = Game.GetFormFromFile(0x00044F93, "SeventySix.esm") as ObjectReference
	ObjectReference roofTrigger = Game.GetFormFromFile(0x004ECEFC, "SeventySix.esm") as ObjectReference

	If akSender == hideoutTrigger && currentStage >= 200 && currentStage < 300
		SetStage(300)
	ElseIf akSender == roofTrigger && currentStage >= 800 && currentStage < 810
		SetStage(810)
	EndIf
EndEvent

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	Int currentStage = GetCurrentStageID()
	If akSender != Game.GetPlayer() || currentStage < 300 || currentStage >= 500
		Return
	EndIf

	CB04_QuestScript mayorQuest = GetMayorQuestScript()
	If mayorQuest == None
		Return
	EndIf

	If akBaseItem == CB04_HolotapeClue1
		mayorQuest.AddClue(1)
	ElseIf akBaseItem == CB04_HolotapeClue2
		mayorQuest.AddClue(2)
	ElseIf akBaseItem == CB04_Password_Note
		mayorQuest.AddClue(4)
	ElseIf akBaseItem == CB04_Keycard
		mayorQuest.AddClue(5)
	EndIf
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
	ObjectReference workTerminalRef = Game.GetFormFromFile(0x00254689, "SeventySix.esm") as ObjectReference
	Int currentStage = GetCurrentStageID()
	If akTerminalRef == workTerminalRef && auiMenuItemID == 0 && currentStage >= 600 && currentStage < 700
		SetStage(700)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf

	If aiTimerID == 10 && GetCurrentStageID() == 10
		ObjectReference officeMarker = Game.GetFormFromFile(0x004E6D7A, "SeventySix.esm") as ObjectReference
		If officeMarker != None && playerRef.GetDistance(officeMarker) <= 600.0
			SetStage(50)
		Else
			StartTimer(2.0, 10)
		EndIf
	ElseIf aiTimerID == 70 && GetCurrentStageID() == 700
		ObjectReference maiaRef = Game.GetFormFromFile(0x002F0515, "SeventySix.esm") as ObjectReference
		If maiaRef != None && playerRef.GetDistance(maiaRef) <= 600.0
			Scene showHolotapeScene = Game.GetFormFromFile(0x004ECEFA, "SeventySix.esm") as Scene
			If showHolotapeScene != None
				showHolotapeScene.Start()
			EndIf
		Else
			StartTimer(2.0, 70)
		EndIf
	EndIf
EndEvent

Function Fragment_Stage_0010_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If Alias_Player.GetReference() != playerRef
			Alias_Player.ForceRefTo(playerRef)
		EndIf
		If CB04_StartedValue != None
			playerRef.SetValue(CB04_StartedValue, 1.0)
		EndIf
	EndIf

	ReconcileClueInventoryListener()
	SetObjectiveDisplayed(10, True)
	StartTimer(2.0, 10)
EndFunction

Function Fragment_Stage_0050_Item_00()
	CancelTimer(10)
	SetObjectiveCompleted(10, True)
	SetObjectiveDisplayed(50, True)
	If CB04_Mayor_Welcome != None
		CB04_Mayor_Welcome.Start()
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveCompleted(50, True)
	SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(200, True)
	RegisterForCB04Trigger(Game.GetFormFromFile(0x00044F93, "SeventySix.esm") as ObjectReference)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(300, True)
	ReconcileClueInventoryListener()
	CountCarriedClues()
EndFunction

Function Fragment_Stage_0500_Item_00()
	ClearClueInventoryListener()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(500, True)
	RegisterForWorkTerminal()
EndFunction

Function Fragment_Stage_0600_Item_00()
	RegisterForWorkTerminal()
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(500, True)
	SetObjectiveDisplayed(700, True)
	CB04_QuestScript mayorQuest = GetMayorQuestScript()
	If mayorQuest != None
		mayorQuest.GiveVirusToPlayer()
	EndIf
	StartTimer(2.0, 70)
EndFunction

Function Fragment_Stage_0800_Item_00()
	CancelTimer(70)
	SetObjectiveCompleted(700, True)
	SetObjectiveDisplayed(800, True)
	RegisterForCB04Trigger(Game.GetFormFromFile(0x004ECEFC, "SeventySix.esm") as ObjectReference)
EndFunction

Function Fragment_Stage_0810_Item_00()
	If CB04_Mayor_RoofA != None
		CB04_Mayor_RoofA.Start()
	EndIf
EndFunction

Function Fragment_Stage_0900_Item_00()
	SetObjectiveCompleted(800, True)
	SetObjectiveDisplayed(900, True)
	If CB04_Mayor_RoofB != None
		CB04_Mayor_RoofB.Start()
	EndIf

	CB04_QuestScript mayorQuest = GetMayorQuestScript()
	If mayorQuest != None
		mayorQuest.StartUploadDefense()
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && PlayerFriendWatogaRobotFaction != None
		playerRef.AddToFaction(PlayerFriendWatogaRobotFaction)
		playerRef.StopCombatAlarm()
	EndIf

	SetObjectiveCompleted(900, True)
	CB04_QuestScript mayorQuest = GetMayorQuestScript()
	If mayorQuest != None
		mayorQuest.FinishUploadDefense()
		mayorQuest.GrantCompletionReward()
	EndIf
	If CB04_Mayor_RoofC != None
		CB04_Mayor_RoofC.Start()
	EndIf
	UnregisterForAllRemoteEvents()
EndFunction
