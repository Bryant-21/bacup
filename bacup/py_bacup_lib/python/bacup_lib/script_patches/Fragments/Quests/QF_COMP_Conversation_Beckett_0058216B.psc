; The visitor's scene sets the wrap-up stage (50) when it completes. The ally's milestone
; handler set the player's visitor value to exactly this conversation's number, and the
; visitor keeps spawning while it matches, so the wrap-up moves it one past the milestone.
Function Fragment_Stage_0000_Item_00()
    Game.GetPlayer().SetValue(COMP_AV_VisitorWaiting_Beckett, 1.0)
EndFunction

Function Fragment_Stage_0050_Item_00()
    Actor playerRef = Game.GetPlayer()
    playerRef.SetValue(COMP_AV_PlayerVisitorValue_Beckett, playerRef.GetValue(COMP_AV_PlayerVisitorValue_Beckett) + 1.0)
    Stop()
EndFunction

Function Fragment_Stage_0100_Item_00()
    Game.GetPlayer().SetValue(COMP_AV_VisitorWaiting_Beckett, 0.0)
EndFunction
