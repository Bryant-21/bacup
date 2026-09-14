; This helper is started by the ChangeLocation Story Manager event, i.e. by the
; player entering the area — the "two women arguing on the side of the mountain"
; start path for Wayward Souls. That path is independent of ever speaking to
; Lacey and Isela, so the Wayward start keyword must be sent from here too;
; otherwise the only way into the quest is the LaceyIselaScene dialogue.
; Wayward is event scoped, so Start()/SetStage() are refused — only a Story
; Manager keyword event can start it. Stage 10 is VMAD-bound on this quest
; (fragments: 10, 100, 1000), so this fragment is genuinely called.
Function Fragment_Stage_0010_Item_00()
    DispatchWaywardStartEvent()
EndFunction

Function DispatchWaywardStartEvent()
    If W05_MQ_001P_Wayward != None && W05_MQ_001P_Wayward_QuestStartKeyword != None
        If !W05_MQ_001P_Wayward.IsRunning() && !W05_MQ_001P_Wayward.IsCompleted()
            ObjectReference playerRef = Game.GetPlayer()
            W05_MQ_001P_Wayward_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
            Debug.Trace("[B21] Wayward start event sent from attract scene; running=" + W05_MQ_001P_Wayward.IsRunning())
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If W05_MQ_001P_Wayward_LaceyIselaAtrractScene_0100_Intro
        W05_MQ_001P_Wayward_LaceyIselaAtrractScene_0100_Intro.Start()
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    If W05_MQ_001P_Wayward_LaceyIselaAtrractScene_0100_Intro
        W05_MQ_001P_Wayward_LaceyIselaAtrractScene_0100_Intro.Stop()
    EndIf
EndFunction
