Function HandleStage(Int aiStage)
    Quest personalQuest = Game.GetFormFromFile(0x003E03AA, "SeventySix.esm") as Quest
    If personalQuest != None && !personalQuest.IsRunning()
        personalQuest.Start()
    EndIf
    (personalQuest as MSiloPersonalQuestScript).HandleStage(aiStage)
EndFunction

Function Fragment_Stage_0010_Item_00()
    HandleStage(10)
EndFunction

Function Fragment_Stage_0011_Item_00()
    HandleStage(11)
EndFunction

Function Fragment_Stage_0019_Item_00()
    HandleStage(19)
EndFunction

Function Fragment_Stage_0100_Item_00()
    HandleStage(100)
EndFunction

Function Fragment_Stage_0110_Item_00()
    HandleStage(110)
EndFunction

Function Fragment_Stage_0120_Item_00()
    HandleStage(120)
EndFunction

Function Fragment_Stage_0130_Item_00()
    HandleStage(130)
EndFunction

Function Fragment_Stage_0140_Item_00()
    HandleStage(140)
EndFunction

Function Fragment_Stage_0150_Item_00()
    HandleStage(150)
EndFunction

Function Fragment_Stage_0160_Item_00()
    HandleStage(160)
EndFunction

Function Fragment_Stage_0170_Item_00()
    HandleStage(170)
EndFunction

Function Fragment_Stage_0180_Item_00()
    HandleStage(180)
EndFunction

Function Fragment_Stage_0198_Item_00()
    HandleStage(198)
EndFunction

Function Fragment_Stage_0200_Item_00()
    HandleStage(200)
EndFunction

Function Fragment_Stage_0210_Item_00()
    HandleStage(210)
EndFunction

Function Fragment_Stage_0220_Item_00()
    HandleStage(220)
EndFunction

Function Fragment_Stage_0230_Item_00()
    HandleStage(230)
EndFunction

Function Fragment_Stage_0240_Item_00()
    HandleStage(240)
EndFunction

Function Fragment_Stage_0250_Item_00()
    HandleStage(250)
EndFunction

Function Fragment_Stage_0298_Item_00()
    HandleStage(298)
EndFunction

Function Fragment_Stage_0300_Item_00()
    HandleStage(300)
EndFunction

Function Fragment_Stage_0310_Item_00()
    HandleStage(310)
EndFunction

Function Fragment_Stage_0320_Item_00()
    HandleStage(320)
EndFunction

Function Fragment_Stage_0398_Item_00()
    HandleStage(398)
EndFunction

Function Fragment_Stage_0400_Item_00()
    HandleStage(400)
EndFunction

Function Fragment_Stage_0410_Item_00()
    HandleStage(410)
EndFunction

Function Fragment_Stage_0419_Item_00()
    HandleStage(419)
EndFunction

Function Fragment_Stage_0420_Item_00()
    HandleStage(420)
EndFunction

Function Fragment_Stage_0430_Item_00()
    HandleStage(430)
EndFunction

Function Fragment_Stage_0440_Item_00()
    HandleStage(440)
EndFunction

Function Fragment_Stage_0498_Item_00()
    HandleStage(498)
EndFunction

Function Fragment_Stage_0500_Item_00()
    HandleStage(500)
EndFunction

Function Fragment_Stage_0510_Item_00()
    HandleStage(510)
EndFunction

Function Fragment_Stage_0520_Item_00()
    HandleStage(520)
EndFunction

Function Fragment_Stage_0530_Item_00()
    HandleStage(530)
EndFunction

Function Fragment_Stage_0598_Item_00()
    HandleStage(598)
EndFunction

Function Fragment_Stage_1000_Item_00()
    HandleStage(1000)
EndFunction
