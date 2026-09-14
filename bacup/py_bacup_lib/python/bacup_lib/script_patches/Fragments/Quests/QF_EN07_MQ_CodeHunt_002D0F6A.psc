EN07_CodeHuntQuestScript Function GetController()
    Return (Self as Quest) as EN07_CodeHuntQuestScript
EndFunction

Function Fragment_Stage_0001_Item_00()
EndFunction

Function Fragment_Stage_0002_Item_00()
EndFunction

Function Fragment_Stage_0003_Item_00()
EndFunction

Function Fragment_Stage_0010_Item_00()
    EN07_CodeHuntQuestScript controller = GetController()
    If controller != None
        controller.HandleStage(10)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor player = Alias_currentPlayer.GetActorReference()
    If player == None
        player = Game.GetPlayer()
    EndIf
    If player != None
        player.SetValue(EN07_CodeHuntFirstTime, 1.0)
    EndIf
    EN07_CodeHuntQuestScript controller = GetController()
    If controller != None
        controller.HandleStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    EN07_CodeHuntQuestScript controller = GetController()
    If controller != None
        controller.HandleStage(200)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    EN07_CodeHuntQuestScript controller = GetController()
    If controller != None
        controller.HandleStage(1000)
    EndIf
EndFunction
