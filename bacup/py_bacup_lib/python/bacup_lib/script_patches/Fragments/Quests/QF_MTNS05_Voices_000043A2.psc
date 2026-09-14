Function Fragment_Stage_0025_Item_00()
    MTNS05_Voices.SetObjectiveDisplayed(25)
EndFunction

Function Fragment_Stage_0050_Item_00()
    MTNS05_Voices.SetObjectiveCompleted(25)
    MTNS05_Voices.SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0050_Item_01()
    MTNS05QuestScript voicesQuest = MTNS05_Voices as MTNS05QuestScript
    If voicesQuest != None
        voicesQuest.ResetTargets()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    MTNS05_Voices.SetObjectiveCompleted(50)
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    ObjectReference holotapeRef = VoxInterpreterHolotape.GetReference()
    If playerRef != None && holotapeRef != None
        playerRef.AddItem(holotapeRef, 1, false)
    EndIf
    MTNS05_Voices.SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0110_Item_00()
    MTNS05_Voices.SetObjectiveCompleted(100)
    MTNS05_Voices.SetStage(150)
EndFunction

Function Fragment_Stage_0150_Item_00()
    ObjectReference syringerRef = Alias_VoxSyringer.GetReference()
    If syringerRef != None
        RegisterForRemoteEvent(syringerRef, "OnContainerChanged")
    EndIf
    MTNS05_Voices.SetObjectiveDisplayed(150)
EndFunction

Function Fragment_Stage_0160_Item_00()
    MTNS05_Voices.SetObjectiveCompleted(150)
    MTNS05_Voices.SetStage(200)
EndFunction

Function Fragment_Stage_0200_Item_00()
    MTNS05QuestScript voicesQuest = MTNS05_Voices as MTNS05QuestScript
    If voicesQuest != None
        voicesQuest.InitializeTargets()
    EndIf
EndFunction

Function Fragment_Stage_0275_Item_00()
    MTNS05_Voices.SetObjectiveDisplayed(275)
EndFunction

Function Fragment_Stage_0300_Item_00()
    MTNS05_Voices.SetObjectiveCompleted(275)
    MTNS05_Voices.SetObjectiveDisplayed(300)
EndFunction

Function Fragment_Stage_0400_Item_00()
    MTNS05_Voices.SetObjectiveCompleted(300)
    ObjectReference syringerRef = Alias_VoxSyringer.GetReference()
    If syringerRef != None
        UnregisterForRemoteEvent(syringerRef, "OnContainerChanged")
    EndIf
    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    ObjectReference holotapeRef = VoxInterpreterHolotape.GetReference()
    If playerRef != None && holotapeRef != None
        playerRef.RemoveItem(holotapeRef, 1, true)
    EndIf
    MTNS05_Voices.Stop()
EndFunction

Event ObjectReference.OnContainerChanged(ObjectReference akSender, ObjectReference akNewContainer, ObjectReference akOldContainer)
    If akSender != Alias_VoxSyringer.GetReference()
        Return
    EndIf

    ObjectReference playerRef = Alias_currentPlayer.GetReference()
    If MTNS05_Voices.GetStage() < 200 && akNewContainer == playerRef
        MTNS05_Voices.SetStage(160)
    ElseIf MTNS05_Voices.GetStage() >= 275 && akNewContainer == Alias_SyringerContainer.GetReference()
        MTNS05_Voices.SetStage(300)
    EndIf
EndEvent
