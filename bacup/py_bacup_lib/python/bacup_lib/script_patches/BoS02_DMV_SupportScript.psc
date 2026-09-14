Event OnQuestInit()
    If pBoS02_DMVNumber_42 != None && pBoS02_DMVNumber_42.GetValue() <= 0.0
        If pBoS02_DeptBCooldown != None
            pBoS02_DeptBCooldown.SetValue(0.0)
        EndIf
        If pBoS02_DMV_Support_100_FirstLoop != None && !pBoS02_DMV_Support_100_FirstLoop.IsPlaying()
            pBoS02_DMV_Support_100_FirstLoop.Start()
        EndIf
    Else
        If pBoS02_DeptCCooldown != None
            pBoS02_DeptCCooldown.SetValue(0.0)
        EndIf
        If pBoS02_DMV_Support_200_SecondLoop != None && !pBoS02_DMV_Support_200_SecondLoop.IsPlaying()
            pBoS02_DMV_Support_200_SecondLoop.Start()
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    If pBoS02_DMV_Support_100_FirstLoop != None && pBoS02_DMV_Support_100_FirstLoop.IsPlaying()
        pBoS02_DMV_Support_100_FirstLoop.Stop()
    EndIf
    If pBoS02_DMV_Support_200_SecondLoop != None && pBoS02_DMV_Support_200_SecondLoop.IsPlaying()
        pBoS02_DMV_Support_200_SecondLoop.Stop()
    EndIf
EndEvent
