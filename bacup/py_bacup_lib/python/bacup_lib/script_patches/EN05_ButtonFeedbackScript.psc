Event OnActivate(ObjectReference akActionRef)
    Actor player = akActionRef as Actor
    If player == None || player != Game.GetPlayer()
        Return
    EndIf

    If EN05_ObstacleQuestActiveKeyword != None && !player.HasKeyword(EN05_ObstacleQuestActiveKeyword)
        If EN05_ObstacleCourse_PlayerLacksQuestMessage != None
            EN05_ObstacleCourse_PlayerLacksQuestMessage.Show()
        EndIf
    ElseIf EN05_ObstacleQuestActiveTargetKeyword != None && GetLinkedRef(EN05_ObstacleQuestActiveTargetKeyword) == None
        If EN05_ObstacleCourse_ButtonInactiveMessage != None
            EN05_ObstacleCourse_ButtonInactiveMessage.Show()
        EndIf
    EndIf
EndEvent
