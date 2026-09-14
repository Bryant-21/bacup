Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(50, True)
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

Function Fragment_Stage_0310_Item_00()
    SetObjectiveDisplayed(310, True)
EndFunction

Function Fragment_Stage_0315_Item_00()
    SetObjectiveCompleted(310, True)
    SFM04_Organic_Radio.Start()
EndFunction

Function Fragment_Stage_0320_Item_00()
    SetObjectiveDisplayed(320, True)
EndFunction

Function Fragment_Stage_0325_Item_00()
    SetObjectiveCompleted(320, True)
EndFunction

Function Fragment_Stage_0340_Item_00()
    SetStage(380)
    TryAdvanceToChemicalDeposit()
EndFunction

Function Fragment_Stage_0380_Item_00()
    SetObjectiveDisplayed(350, True)
    SetObjectiveDisplayed(360, True)
    SetObjectiveDisplayed(370, True)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(350, True)
    TryAdvanceToChemicalDeposit()
EndFunction

Function Fragment_Stage_0360_Item_00()
    SetObjectiveCompleted(360, True)
    TryAdvanceToChemicalDeposit()
EndFunction

Function Fragment_Stage_0370_Item_00()
    SetObjectiveCompleted(370, True)
    TryAdvanceToChemicalDeposit()
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0410_Item_00()
    If IsStageDone(420) && IsStageDone(430)
        SetStage(435)
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    If IsStageDone(410) && IsStageDone(430)
        SetStage(435)
    EndIf
EndFunction

Function Fragment_Stage_0430_Item_00()
    If IsStageDone(410) && IsStageDone(420)
        SetStage(435)
    EndIf
EndFunction

Function Fragment_Stage_0435_Item_00()
    If IsStageDone(440)
        SetStage(450)
    EndIf
EndFunction

Function Fragment_Stage_0440_Item_00()
    If IsStageDone(435)
        SetStage(450)
    EndIf
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(475, True)
EndFunction

Function Fragment_Stage_0460_Item_00()
    SetObjectiveCompleted(475, True)
    SetObjectiveDisplayed(500, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(500, True)
    SetObjectiveDisplayed(600, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(600, True)
    SetObjectiveDisplayed(700, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(700, True)
    Actor playerRef = Alias_SFM04Player.GetActorReference()
    If playerRef != None
        If playerRef.GetItemCount(SFM04_Organic_RadShield) <= 0
            playerRef.AddItem(SFM04_Organic_RadShield, 1, False)
        EndIf
        If playerRef.GetItemCount(SFM04_Organic_RadShield) > 0 && !IsStageDone(1000)
            SetStage(1000)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    Stop()
EndFunction
Function TryAdvanceToChemicalDeposit()
    If IsStageDone(340) && IsStageDone(350) && IsStageDone(360) && IsStageDone(370) && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction
