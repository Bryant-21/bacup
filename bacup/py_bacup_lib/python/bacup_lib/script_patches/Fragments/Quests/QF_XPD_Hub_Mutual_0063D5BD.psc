Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0200_Item_00()
    If AttractScene != None && !AttractScene.IsPlaying()
        AttractScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(10)
    StopAttractScene()
EndFunction

Function Fragment_Stage_0300_Item_00()
    StopAttractScene()
EndFunction

Function Fragment_Stage_0400_Item_00()
    DisplayDonationObjectives(400)
EndFunction

Function Fragment_Stage_0500_Item_00()
    DisplayDonationObjectives(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    DisplayDonationObjectives(600)
EndFunction

Function Fragment_Stage_0700_Item_00()
    DisplayDonationObjectives(700)
EndFunction

Function Fragment_Stage_0800_Item_00()
    DisplayDonationObjectives(800)
EndFunction

Function Fragment_Stage_0900_Item_00()
    DisplayDonationObjectives(900)
EndFunction

Function Fragment_Stage_1000_Item_00()
    DisplayDonationObjectives(1000)
EndFunction

Function Fragment_Stage_8000_Item_00()
    StopAttractScene()
EndFunction

Function Fragment_Stage_9000_Item_00()
    StopAttractScene()
    CompleteAllObjectives()
EndFunction

Function DisplayDonationObjectives(Int objectiveIndex)
    SetObjectiveDisplayed(objectiveIndex)
EndFunction

Function StopAttractScene()
    If AttractScene != None && AttractScene.IsPlaying()
        AttractScene.Stop()
    EndIf
EndFunction
