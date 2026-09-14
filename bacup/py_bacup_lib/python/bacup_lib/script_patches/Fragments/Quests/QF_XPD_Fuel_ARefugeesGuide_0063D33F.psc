Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(2)
EndFunction

Function Fragment_Stage_0101_Item_00()
EndFunction

Function Fragment_Stage_0104_Item_00()
    SetObjectiveCompleted(2)
    SetObjectiveDisplayed(5)
EndFunction

Function Fragment_Stage_0105_Item_00()
EndFunction

Function Fragment_Stage_0106_Item_00()
EndFunction

Function Fragment_Stage_0109_Item_00()
    SetObjectiveCompleted(5)
EndFunction

Function Fragment_Stage_0200_Item_00()
    DisplayRegionObjectives(9, 10)
EndFunction

Function Fragment_Stage_0201_Item_00()
    DisplayRegionObjectives(59, 60)
EndFunction

Function Fragment_Stage_0202_Item_00()
    DisplayRegionObjectives(109, 110)
EndFunction

Function Fragment_Stage_0203_Item_00()
    DisplayRegionObjectives(259, 260)
EndFunction

Function Fragment_Stage_0204_Item_00()
    DisplayRegionObjectives(159, 160)
EndFunction

Function Fragment_Stage_0205_Item_00()
    DisplayRegionObjectives(309, 310)
EndFunction

Function Fragment_Stage_0206_Item_00()
    DisplayRegionObjectives(209, 210)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(10)
EndFunction

Function Fragment_Stage_0820_Item_00()
    SetObjectiveCompleted(30)
EndFunction

Function Fragment_Stage_0830_Item_00()
    SetObjectiveCompleted(40)
EndFunction

Function Fragment_Stage_0840_Item_00()
    SetObjectiveCompleted(50)
EndFunction

Function Fragment_Stage_0850_Item_00()
    SetObjectiveCompleted(60)
EndFunction

Function Fragment_Stage_0870_Item_00()
    SetObjectiveCompleted(80)
EndFunction

Function Fragment_Stage_0880_Item_00()
    SetObjectiveCompleted(90)
EndFunction

Function Fragment_Stage_0890_Item_00()
    SetObjectiveCompleted(100)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveCompleted(110)
EndFunction

Function Fragment_Stage_0920_Item_00()
    SetObjectiveCompleted(130)
EndFunction

Function Fragment_Stage_0930_Item_00()
    SetObjectiveCompleted(140)
EndFunction

Function Fragment_Stage_0940_Item_00()
    SetObjectiveCompleted(150)
EndFunction

Function Fragment_Stage_0950_Item_00()
    SetObjectiveCompleted(260)
EndFunction

Function Fragment_Stage_0970_Item_00()
    SetObjectiveCompleted(280)
EndFunction

Function Fragment_Stage_0980_Item_00()
    SetObjectiveCompleted(290)
EndFunction

Function Fragment_Stage_0990_Item_00()
    SetObjectiveCompleted(300)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(160)
EndFunction

Function Fragment_Stage_1020_Item_00()
    SetObjectiveCompleted(180)
EndFunction

Function Fragment_Stage_1030_Item_00()
    SetObjectiveCompleted(190)
EndFunction

Function Fragment_Stage_1040_Item_00()
    SetObjectiveCompleted(200)
EndFunction

Function Fragment_Stage_1050_Item_00()
    SetObjectiveCompleted(210)
EndFunction

Function Fragment_Stage_1070_Item_00()
    SetObjectiveCompleted(230)
EndFunction

Function Fragment_Stage_1080_Item_00()
    SetObjectiveCompleted(240)
EndFunction

Function Fragment_Stage_1090_Item_00()
    SetObjectiveCompleted(250)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(310)
EndFunction

Function Fragment_Stage_1120_Item_00()
    SetObjectiveCompleted(340)
EndFunction

Function Fragment_Stage_1130_Item_00()
    SetObjectiveCompleted(330)
EndFunction

Function Fragment_Stage_1140_Item_00()
    SetObjectiveCompleted(350)
EndFunction

Function Fragment_Stage_2000_Item_00()
    CompleteRegionHeaders()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(500)
    CompleteAllObjectives()
EndFunction

Function Fragment_Stage_9990_Item_00()
    Stop()
EndFunction

Function DisplayRegionObjectives(Int headerObjective, Int harvestObjective)
    SetObjectiveDisplayed(headerObjective)
    SetObjectiveDisplayed(harvestObjective)
EndFunction

Function CompleteRegionHeaders()
    SetObjectiveCompleted(9)
    SetObjectiveCompleted(59)
    SetObjectiveCompleted(109)
    SetObjectiveCompleted(159)
    SetObjectiveCompleted(209)
    SetObjectiveCompleted(259)
    SetObjectiveCompleted(309)
EndFunction
