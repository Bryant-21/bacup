Function SetPlayerFlag(ActorValue akFlag)
    Actor playerActor = Alias_Player.GetActorReference()
    If playerActor != None
        playerActor.SetValue(akFlag, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetPlayerFlag(D01_Brewing_Tutorial_Furniture)
    D01_Tutorial_Brewing01.Show()
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetPlayerFlag(D01_Brewing_Tutorial_Brewed)
    D01_Tutorial_Brewing02.Show()
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetPlayerFlag(D01_Brewing_Tutorial_Fermenter)
    D01_Tutorial_Brewing03.Show()
EndFunction

Function Fragment_Stage_0400_Item_00()
    D01_Tutorial_Brewing04.Show()
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetPlayerFlag(D01_Brewing_Tutorial_Vintage)
    D01_Tutorial_Brewing05.Show()
EndFunction

Function Fragment_Stage_0999_Item_00()
    Stop()
EndFunction
