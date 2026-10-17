; Stage 10 is set by ENB_ScienceWing_Intro_Scene (2B8000) SCQS OnEnd. ENB_ScienceWingIntroQuestNode
; only starts the intro while the triggering player's ENB_ScienceWingIntro_HeardOnce is 0.
Function Fragment_Stage_0010_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    playerRef.SetValue(ENB_ScienceWingIntro_HeardOnce, 1.0)
EndFunction
