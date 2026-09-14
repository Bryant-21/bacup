Event OnQuestInit()
    If Initialized
        Return
    EndIf

    ; FO76 rotates this state on the server. FO4 keeps one local code cycle.
    checkCodeResetBusy = False
    CurrentCodeIndex = 0
    If SQ_TimestampToday != None
        currentCodeStartTimestamp = SQ_TimestampToday.GetValue()
    Else
        currentCodeStartTimestamp = 0.0
    EndIf
    NukeCodesSolutionAvailable = False
    NukeCodesActive = True

    Actor player = Game.GetPlayer()
    If player != None
        If Nuke_CodeIndexAV != None
            player.SetValue(Nuke_CodeIndexAV, 0.0)
        EndIf
        If Nuke_CodeYearAV != None
            player.SetValue(Nuke_CodeYearAV, 0.0)
        EndIf
    EndIf

    Initialized = True
    If Nuke_CodesStartQuest != None
        Nuke_CodesStartQuest.SendStoryEvent(None, player, player)
    EndIf
EndEvent
