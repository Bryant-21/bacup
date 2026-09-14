Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        If Alias_Player != None
            Alias_Player.ForceRefIfEmpty(playerRef)
        EndIf
        If Alias_Player_KeywordRef != None
            Alias_Player_KeywordRef.ForceRefIfEmpty(playerRef)
        EndIf
        If BS01_RussellDorsey_DialogueGate_AV != None
            playerRef.SetValue(BS01_RussellDorsey_DialogueGate_AV, 1.0)
        EndIf
    EndIf
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0110_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && BS01_RussellDorsey_DialogueGate_AV != None
        playerRef.SetValue(BS01_RussellDorsey_DialogueGate_AV, 0.0)
    EndIf
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0120_Item_00()
    If Alias_Actors_AtlasInitiates != None && Alias_Actors_AtlasInitiates_Keyword != None
        Int initiateIndex = 0
        While initiateIndex < Alias_Actors_AtlasInitiates.GetCount()
            ObjectReference initiateRef = Alias_Actors_AtlasInitiates.GetAt(initiateIndex)
            If initiateRef != None && Alias_Actors_AtlasInitiates_Keyword.Find(initiateRef) < 0
                Alias_Actors_AtlasInitiates_Keyword.AddRef(initiateRef)
            EndIf
            initiateIndex += 1
        EndWhile
    EndIf

    ObjectReference shinRef = Alias_NPC_Shin.GetReference()
    ObjectReference shinMarker = Alias_Marker_ShinIntro.GetReference()
    If shinRef != None && shinMarker != None
        shinRef.MoveTo(shinMarker)
    EndIf

    ObjectReference rahmaniRef = Alias_NPC_Rahmani.GetReference()
    ObjectReference rahmaniMarker = Alias_Marker_RahmaniIntro.GetReference()
    If rahmaniRef != None && rahmaniMarker != None
        rahmaniRef.MoveTo(rahmaniMarker)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveDisplayed(200, False)
    SetObjectiveDisplayed(210)
EndFunction

Function Fragment_Stage_0160_Item_00()
    If BS01_MQ01_Trust_IntroScene != None && !BS01_MQ01_Trust_IntroScene.IsPlaying()
        BS01_MQ01_Trust_IntroScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0170_Item_00()
    SetObjectiveCompleted(210)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0180_Item_00()
    ObjectReference bankDoor = Alias_Door_FortBankDoor.GetReference()
    If bankDoor != None
        bankDoor.SetOpen(False)
        bankDoor.Lock(True)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300)
    ; FO4 has no bound declarations for the FO76 per-player objective counters.
EndFunction

Function Fragment_Stage_0210_Item_00()
    If IsStageDone(210) && IsStageDone(220) && IsStageDone(230) && IsStageDone(240) && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    If IsStageDone(210) && IsStageDone(220) && IsStageDone(230) && IsStageDone(240) && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0230_Item_00()
    If IsStageDone(210) && IsStageDone(220) && IsStageDone(230) && IsStageDone(240) && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0240_Item_00()
    If IsStageDone(210) && IsStageDone(220) && IsStageDone(230) && IsStageDone(240) && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(500)

    ObjectReference laserGrid = Alias_Activator_LaserGrid.GetReference()
    If laserGrid != None && !laserGrid.IsDisabled()
        laserGrid.Disable()
    EndIf

    ObjectReference bankDoor = Alias_Door_FortBankDoor.GetReference()
    If bankDoor != None
        bankDoor.Lock(False)
        bankDoor.SetOpen(True)
    EndIf
    ; The stripped PEX declares BS01_NewBrothers_MachineScene, but QUST VMAD does not bind it.
    ; FO4's DefaultAliasOnOpen is player-only, so SetOpen(True) cannot deliver this transition.
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteQuest()
    If BS01_TryStartInvention()
        Stop()
    Else
        StartTimer(5.0, 9000)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 9000 || !IsStageDone(9000)
        Return
    EndIf

    If BS01_TryStartInvention()
        Stop()
    Else
        StartTimer(5.0, 9000)
    EndIf
EndEvent

Bool Function BS01_TryStartInvention()
    Quest inventionQuest = Game.GetFormFromFile(0x005B79EB, "SeventySix.esm") as Quest
    If inventionQuest != None && (inventionQuest.IsRunning() || inventionQuest.IsCompleted())
        Return True
    EndIf

    ObjectReference playerRef = None
    If Alias_Player != None
        playerRef = Alias_Player.GetReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None || BS01_Invention_QuestStartKeyword == None
        Return False
    EndIf

    Bool accepted = BS01_Invention_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    Return accepted || (inventionQuest != None && (inventionQuest.IsRunning() || inventionQuest.IsCompleted()))
EndFunction
