Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    If !IsStageDone(300)
        SetObjectiveDisplayed(20, True)
    EndIf
    SetObjectiveDisplayed(999, True)
    ObjectReference boxRef = Alias_container_HolotapeBox.GetReference()
    If boxRef != None && ShelleyHolo01 != None && alias_ShelleyHolo01.GetReference() == None
        ObjectReference holotapeRef = boxRef.PlaceAtMe(ShelleyHolo01, 1, True)
        If holotapeRef != None
            alias_ShelleyHolo01.ForceRefTo(holotapeRef)
            boxRef.AddItem(holotapeRef, 1, True)
        EndIf
    EndIf
    SetStage(250)
EndFunction

Function Fragment_Stage_0250_Item_00()
    ObjectReference holotapeRef = alias_ShelleyHolo01.GetReference()
    If holotapeRef != None
        holotapeRef.Enable(False)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveCompleted(25, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(45, True)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(45, True)
    SetObjectiveDisplayed(50, True)
    If !IsStageDone(5003)
        SetObjectiveDisplayed(60, True)
    ElseIf !IsStageDone(5005)
        SetObjectiveDisplayed(70, True)
    EndIf
EndFunction

Function Fragment_Stage_0625_Item_00()
    If IsObjectiveDisplayed(60)
        SetObjectiveCompleted(60, True)
    EndIf
    If IsObjectiveDisplayed(70)
        SetObjectiveCompleted(70, True)
    EndIf
    If !IsStageDone(700)
        SetObjectiveDisplayed(80, True)
    EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(80, True)
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && Clue_Bone != None && playerRef.GetItemCount(Clue_Bone) > 0
        playerRef.RemoveItem(Clue_Bone, 1, True)
    EndIf
    ObjectReference mountedBone = Alias_static_Bone.GetReference()
    If mountedBone != None
        mountedBone.Enable(False)
    EndIf
    ObjectReference safeRef = Alias_container_WolfSafe.GetReference()
    If safeRef != None && Password_Terminal != None && Alias_alias_Password_Terminal.GetReference() == None
        ObjectReference codeRef = safeRef.PlaceAtMe(Password_Terminal, 1, True)
        If codeRef != None
            Alias_alias_Password_Terminal.ForceRefTo(codeRef)
            safeRef.AddItem(codeRef, 1, True)
        EndIf
    EndIf
    SetStage(750)
EndFunction

Function Fragment_Stage_0750_Item_00()
    If !IsStageDone(775)
        SetObjectiveDisplayed(85, True)
    EndIf
EndFunction

Function Fragment_Stage_0775_Item_00()
    SetObjectiveCompleted(85, True)
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(90, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(90, True)
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(100, True)
    SetStage(950)
EndFunction

Function Fragment_Stage_0950_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If StartKeyword_BBBS != None && playerRef != None
        StartKeyword_BBBS.SendStoryEvent(None, playerRef)
    EndIf
    SetStage(9000)
EndFunction

Function Fragment_Stage_5000_Item_00()
    SetObjectiveCompleted(999, True)
EndFunction

Function Fragment_Stage_5001_Item_00()
    If !IsStageDone(300)
        SetObjectiveCompleted(20, True)
        SetObjectiveDisplayed(25, True)
    EndIf
EndFunction

Function Fragment_Stage_5002_Item_00()
    UpdateClueCount()
EndFunction

Function Fragment_Stage_5003_Item_00()
    UpdateClueCount()
    If IsStageDone(550) && !IsStageDone(625)
        SetObjectiveCompleted(60, True)
        SetObjectiveDisplayed(70, True)
    EndIf
EndFunction

Function Fragment_Stage_5004_Item_00()
    UpdateClueCount()
EndFunction

Function Fragment_Stage_5005_Item_00()
    UpdateClueCount()
    SetStage(625)
EndFunction

Function Fragment_Stage_9000_Item_00()
    If !IsObjectiveCompleted(999)
        SetObjectiveDisplayed(999, False)
    EndIf
EndFunction

Function UpdateClueCount()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef == None || AV_CluesCurrent == None
        Return
    EndIf
    Int found = 0
    Int clueStage = 5002
    While clueStage <= 5005
        If IsStageDone(clueStage)
            found += 1
        EndIf
        clueStage += 1
    EndWhile
    playerRef.SetValue(AV_CluesCurrent, found as Float)
EndFunction
