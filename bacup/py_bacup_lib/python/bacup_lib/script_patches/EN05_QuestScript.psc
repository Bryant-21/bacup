Function EN05Basic_CourseCompleted(Int aiCourseID)
    If !IsRunning()
        Return
    EndIf

    Int stageToSet = -1
    If aiCourseID == iMarkmanshipCourseID
        stageToSet = iMarksmanshipCompleteStage
    ElseIf aiCourseID == iObstacleCourseID
        stageToSet = iObstacleCompleteStage
    ElseIf aiCourseID == iPatriotismCourseID
        stageToSet = iPatriotismCompleteStage
    ElseIf aiCourseID == iCombatCourseID
        If IsStageDone(iInitialCoursesCompletedStage)
            stageToSet = iCombatCompleteStage
        Else
            stageToSet = iCombatCompletedEarlyStage
        EndIf
    EndIf

    If stageToSet >= 0 && !IsStageDone(stageToSet)
        SetStage(stageToSet)
    EndIf
EndFunction
