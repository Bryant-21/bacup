; Emerson's optional vendor visit plays its scene once per player (COMP_AV_Astronaut_EmersonVendorSceneOnce) and the
; conversation quest stops when the scene ends or has already been seen.
Event OnQuestInit()
    Actor player = PlayerAlias.GetActorReference()
    If player == None
        player = Game.GetPlayer()
    EndIf
    If player.GetValue(COMP_AV_Astronaut_EmersonVendorSceneOnce) > 0.0 || COMP_Conversation_Astronaut_SceneEmerson_017_Optional == None
        Stop()
        Return
    EndIf
    player.SetValue(COMP_AV_Astronaut_EmersonVendorSceneOnce, 1.0)
    RegisterForRemoteEvent(COMP_Conversation_Astronaut_SceneEmerson_017_Optional, "OnEnd")
    COMP_Conversation_Astronaut_SceneEmerson_017_Optional.Start()
EndEvent

Event Scene.OnEnd(Scene akSender)
    UnregisterForRemoteEvent(akSender, "OnEnd")
    Stop()
EndEvent
