; RD01 Gleaming Depths master stages. Contract: bacup/docs/stub_restoration/contracts/
; rd01-raid-shell-lifecycle-2026-09-23.md §2.2. The Tales raid controller sets the start and owner
; stages; these bodies only drive objectives, the checkpoint alias and the exit doors.
; Objective pairs: 10/15 Enc01, 20/25 Enc02, 30/35 Enc04, 50/55 Enc05, 60/65 Enc06.

Function Fragment_Stage_0002_Item_00()
	SetStage(200)
EndFunction

Function Fragment_Stage_0004_Item_00()
	SetStage(200)
	SetStage(300)
EndFunction

Function Fragment_Stage_0005_Item_00()
	SetStage(200)
	SetStage(300)
	SetStage(500)
EndFunction

Function Fragment_Stage_0006_Item_00()
	SetStage(200)
	SetStage(300)
	SetStage(500)
	SetStage(600)
EndFunction

Function Fragment_Stage_0007_Item_00()
EndFunction

Function Fragment_Stage_0010_Item_00()
	If !IsStageDone(200)
		RedisplayEncounterObjectives(10, 175)
	ElseIf !IsStageDone(300)
		RedisplayEncounterObjectives(20, 250)
	ElseIf !IsStageDone(500)
		RedisplayEncounterObjectives(30, 350)
	ElseIf !IsStageDone(600)
		RedisplayEncounterObjectives(50, 550)
	ElseIf !IsStageDone(700)
		RedisplayEncounterObjectives(60, 650)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0150_Item_00()
	If PASystem_Hallway01_Topic != None
		Game.GetPlayer().Say(PASystem_Hallway01_Topic, None, True)
	EndIf
EndFunction

Function Fragment_Stage_0175_Item_00()
	StartEncounterCheckpoint(15, Alias_RespawnMarkers_Enc01)
EndFunction

Function Fragment_Stage_0200_Item_00()
	CompleteEncounter(10, Alias_ExitDoor_Enc01, 20)
EndFunction

Function Fragment_Stage_0250_Item_00()
	StartEncounterCheckpoint(25, Alias_RespawnMarkers_Enc02)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20)
	SetObjectiveCompleted(25)
	SetCollectionEnabled(Alias_Enc02_Exit, True)
	SetCollectionEnabled(Alias_Enc02_ExitFX, False)
	SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0350_Item_00()
	StartEncounterCheckpoint(35, Alias_RespawnMarkers_Enc04)
EndFunction

Function Fragment_Stage_0500_Item_00()
	CompleteEncounter(30, Alias_ExitDoor_Enc04, 50)
EndFunction

Function Fragment_Stage_0550_Item_00()
	StartEncounterCheckpoint(55, Alias_RespawnMarkers_Enc05)
EndFunction

Function Fragment_Stage_0600_Item_00()
	CompleteEncounter(50, Alias_ExitDoor_Enc05, 60)
EndFunction

Function Fragment_Stage_0650_Item_00()
	StartEncounterCheckpoint(65, Alias_RespawnMarkers_Enc06)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(60)
	SetObjectiveCompleted(65)
	SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
EndFunction

Function StartEncounterCheckpoint(Int aiDefeatObjective, RefCollectionAlias akRespawnMarkers)
	SetObjectiveCompleted(aiDefeatObjective - 5)
	SetObjectiveDisplayed(aiDefeatObjective)
	If akRespawnMarkers != None && akRespawnMarkers.GetCount() > 0 && akRespawnMarkers.GetAt(0) != None
		Alias_CurrentRespawnMarker.ForceRefTo(akRespawnMarkers.GetAt(0))
		If CheckpointMessage != None
			CheckpointMessage.Show()
		EndIf
	EndIf
EndFunction

Function CompleteEncounter(Int aiExploreObjective, ReferenceAlias akExitDoor, Int aiNextExploreObjective)
	SetObjectiveCompleted(aiExploreObjective)
	SetObjectiveCompleted(aiExploreObjective + 5)
	ObjectReference exitDoor = None
	If akExitDoor != None
		exitDoor = akExitDoor.GetReference()
	EndIf
	If exitDoor != None
		exitDoor.Lock(False)
		exitDoor.SetOpen(True)
	EndIf
	SetObjectiveDisplayed(aiNextExploreObjective)
EndFunction

Function RedisplayEncounterObjectives(Int aiExploreObjective, Int aiStartStage)
	If IsStageDone(aiStartStage)
		SetObjectiveCompleted(aiExploreObjective)
		SetObjectiveDisplayed(aiExploreObjective + 5)
	Else
		SetObjectiveDisplayed(aiExploreObjective)
	EndIf
EndFunction

Function SetCollectionEnabled(RefCollectionAlias akCollection, Bool abEnabled)
	If akCollection == None
		Return
	EndIf
	Int index = 0
	While index < akCollection.GetCount()
		ObjectReference ref = akCollection.GetAt(index)
		If ref != None
			If abEnabled
				ref.Enable()
			Else
				ref.Disable()
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction
