; FO76's on-connect node (7F79DC) also required GetLevel >= 30. That PCON node is not
; converted (this quest autostarts instead), so the level gate lives here; players below
; 30 are picked up later by BURN_SQ01_LevelIncrease, whose LEVL node keeps the gate.
Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_Player.GetActorReference()
    If playerRef != None && playerRef.GetLevel() >= 30 && BURN_SQ01 != None && BURN_SQ01_QuestStartKeyword != None
        If !BURN_SQ01.IsRunning() && !BURN_SQ01.IsCompleted()
            BURN_SQ01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
        EndIf
    EndIf
    Stop()
EndFunction
