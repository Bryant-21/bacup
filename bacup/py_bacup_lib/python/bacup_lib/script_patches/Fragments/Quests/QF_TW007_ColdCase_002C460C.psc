Function Fragment_Stage_0001_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0099_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0225_Item_00()
    If IsStageDone(250)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    If IsStageDone(225)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(22, True)
EndFunction

Function Fragment_Stage_0350_Item_00()
    If IsStageDone(375)
        SetStage(400)
    Else
        SetStage(380)
    EndIf
EndFunction

Function Fragment_Stage_0375_Item_00()
    If IsStageDone(350)
        SetStage(400)
    Else
        SetStage(380)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(22, True)
    SetObjectiveDisplayed(24, True)
    SetObjectiveDisplayed(26, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(24, True)
    SetObjectiveDisplayed(100, True)
    If IsStageDone(550)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(26, True)
    If IsStageDone(500)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(24, True)
    SetObjectiveCompleted(26, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(65, True)
EndFunction

Function Fragment_Stage_1300_Item_00()
    TW007_PlayerScript playerScript = Alias_Player as TW007_PlayerScript
    If playerScript != None
        playerScript.ReconcileFreddyClues()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(65, True)
    SetObjectiveDisplayed(110, True)
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(110, True)
    SetObjectiveDisplayed(80, True)
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(80, True)
    Stop()
EndFunction

Function Fragment_Stage_2000_Item_00()
    Stop()
EndFunction
