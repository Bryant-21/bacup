Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0175_Item_00()
    If !IsStageDone(10) && !IsStageDone(20) && !IsStageDone(30)
        SetStage(20)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10)
    If IsStageDone(10)
        SetStage(300)
    ElseIf IsStageDone(20)
        SetStage(400)
    ElseIf IsStageDone(30)
        SetStage(500)
    Else
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    ObjectReference bandageRef = Alias_MiscItem_BandageToEnable.GetReference()
    ObjectReference foodParcelRef = Alias_MiscItem_FoodParcelToEnable.GetReference()
    ObjectReference missiveRef = Alias_MiscItem_MissiveToEnable.GetReference()
    If bandageRef != None
        bandageRef.Enable()
    EndIf
    If foodParcelRef != None
        foodParcelRef.Enable()
    EndIf
    If missiveRef != None
        missiveRef.Enable()
    EndIf
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0310_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30, True, True)
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveCompleted(30)
    SetObjectiveDisplayed(40, True, True)
EndFunction

Function Fragment_Stage_0330_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        Int carePackageCount = playerRef.GetItemCount(NPE_DQ01_CarePackage)
        If carePackageCount > CarePkgTotal
            carePackageCount = CarePkgTotal
        EndIf
        If carePackageCount > 0
            playerRef.RemoveItem(NPE_DQ01_CarePackage, carePackageCount, True)
        EndIf
    EndIf
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(50, True, True)
EndFunction

Function Fragment_Stage_0410_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(51, True, True)
EndFunction

Function Fragment_Stage_0415_Item_00()
    Return
EndFunction

Function Fragment_Stage_0420_Item_00()
    SetObjectiveCompleted(51)
    SetObjectiveDisplayed(52, True, True)
EndFunction

Function Fragment_Stage_0430_Item_00()
    SetObjectiveCompleted(52)
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_0505_Item_00()
    SetObjectiveDisplayed(60, True, True)
EndFunction

Function Fragment_Stage_0510_Item_00()
    SetObjectiveCompleted(60)
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None
        If IsStageDone(430)
            Int meatCount = playerRef.GetItemCount(NPE_DQ01_FreshMeat)
            If meatCount > 3
                meatCount = 3
            EndIf
            If meatCount > 0
                playerRef.RemoveItem(NPE_DQ01_FreshMeat, meatCount, True)
            EndIf
        ElseIf IsStageDone(510)
            Int artifactCount = playerRef.GetItemCount(NPE_DQ01_CultistArtifact)
            If artifactCount > ArtifactsTotal
                artifactCount = ArtifactsTotal
            EndIf
            If artifactCount > 0
                playerRef.RemoveItem(NPE_DQ01_CultistArtifact, artifactCount, True)
            EndIf
        EndIf
    EndIf
    SetObjectiveDisplayed(70, True, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(70)
    SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Stop()
EndFunction
