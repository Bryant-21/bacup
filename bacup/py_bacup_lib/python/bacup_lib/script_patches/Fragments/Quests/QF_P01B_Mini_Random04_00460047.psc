Function Fragment_Stage_0010_Item_00()
    P01B_Mini_Random04_Script questController = (Self as Quest) as P01B_Mini_Random04_Script
    If questController != None
        questController.RefreshClueProgress()
    EndIf
EndFunction

Function Fragment_Stage_0011_Item_00()
    P01B_Mini_Random04_Script questController = (Self as Quest) as P01B_Mini_Random04_Script
    If questController != None
        questController.RefreshClueProgress()
    EndIf
EndFunction

Function Fragment_Stage_0012_Item_00()
    P01B_Mini_Random04_Script questController = (Self as Quest) as P01B_Mini_Random04_Script
    If questController != None
        questController.RefreshClueProgress()
    EndIf
EndFunction

Function Fragment_Stage_0013_Item_00()
    P01B_Mini_Random04_Script questController = (Self as Quest) as P01B_Mini_Random04_Script
    If questController != None
        questController.RefreshClueProgress()
    EndIf
EndFunction

Function Fragment_Stage_0021_Item_00()
    P01B_Mini_Random04_Script questController = (Self as Quest) as P01B_Mini_Random04_Script
    If questController != None
        questController.RefreshClueProgress()
    EndIf
EndFunction

Function Fragment_Stage_0022_Item_00()
    P01B_Mini_Random04_Script questController = (Self as Quest) as P01B_Mini_Random04_Script
    If questController != None
        questController.RefreshClueProgress()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None
        If P01B_AV_CaseStatus != None
            playerRef.SetValue(P01B_AV_CaseStatus, 1.0)
        EndIf
        If P01B_Mini_Random04_CluesFound != None
            playerRef.SetValue(P01B_Mini_Random04_CluesFound, 0.0)
        EndIf
        If P01B_Mini_Random04_CluesFoundLoc1 != None
            playerRef.SetValue(P01B_Mini_Random04_CluesFoundLoc1, 0.0)
        EndIf
        If P01B_Mini_Random04_CluesFoundLoc2 != None
            playerRef.SetValue(P01B_Mini_Random04_CluesFoundLoc2, 0.0)
        EndIf
        If P01B_Mini_Random04_CluesFoundLoc3 != None
            playerRef.SetValue(P01B_Mini_Random04_CluesFoundLoc3, 0.0)
        EndIf
    EndIf
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    P01B_Mini_Random04_Script questController = (Self as Quest) as P01B_Mini_Random04_Script
    If questController != None
        questController.RefreshClueProgress()
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    If playerRef != None && P01B_AV_CaseStatus != None
        playerRef.SetValue(P01B_AV_CaseStatus, 2.0)
    EndIf
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    Actor playerRef = Alias_Player.GetReference() as Actor
    Quest masterQuest = Game.GetFormFromFile(0x0047F444, "SeventySix.esm") as Quest
    SetObjectiveCompleted(100, True)
    SetObjectiveCompleted(200, True)
    SetObjectiveCompleted(300, True)
    CompleteQuest()
    If playerRef != None && P01B_AV_CaseStatus != None
        playerRef.SetValue(P01B_AV_CaseStatus, 3.0)
    EndIf
    If masterQuest != None
        If IsStageDone(550)
            masterQuest.SetStage(6010)
        Else
            masterQuest.SetStage(6000)
        EndIf
    EndIf
EndFunction
