; Each phase objective names the LCP toggle bound to it: week1 LCP_SDOW_Slasher,
; week3 DailyOps, Week2 GraveDigging, Week4 Infestation, Week5 MischiefNight,
; week7 SpookySlasher, week9 HeadHunts.
Function Fragment_Stage_0100_Item_00()
    DisplayPhaseObjective(10, week1)
    DisplayPhaseObjective(20, week3)
    DisplayPhaseObjective(30, Week2)
    DisplayPhaseObjective(40, Week4)
    DisplayPhaseObjective(50, Week5)
    DisplayPhaseObjective(60, week7)
    DisplayPhaseObjective(70, week9)
EndFunction

Function DisplayPhaseObjective(Int aiObjective, GlobalVariable akToggle)
    If akToggle != None && akToggle.GetValue() >= 1.0 && !IsObjectiveDisplayed(aiObjective)
        SetObjectiveDisplayed(aiObjective)
    EndIf
EndFunction
