; The visitor's scene sets the wrap-up stage when it completes. The ally's milestone
; handler set the player's visitor value to exactly this conversation's number, and the
; visitor keeps spawning while it matches, so the wrap-up moves it one past the milestone.
Function Fragment_Stage_0000_Item_00()
    Game.GetPlayer().SetValue(AV_VisitorWaiting, 1.0)
EndFunction

Function Fragment_Stage_0010_Item_00()
    If !COMP_Conversation_Astronaut_Emerson_008_Revelations_ApproachScene.IsPlaying()
        COMP_Conversation_Astronaut_Emerson_008_Revelations_ApproachScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    If COMP_AV_PlayerVisitorValue_Astronaut != None
        playerRef.SetValue(COMP_AV_PlayerVisitorValue_Astronaut, playerRef.GetValue(COMP_AV_PlayerVisitorValue_Astronaut) + 1.0)
    EndIf
    playerRef.ModValue(AV_Emerson_Visits, 1.0)
    playerRef.SetValue(AV_VisitorWaiting, 0.0)
    Stop()
EndFunction
