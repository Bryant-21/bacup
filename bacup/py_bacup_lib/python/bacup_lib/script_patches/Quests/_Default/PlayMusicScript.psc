Event OnQuestInit()
    B21MusicPlaying = None
    RefreshMusic(False)
    StartTimer(2.0, 7631)
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    RefreshMusic(False)
EndEvent

; Participation can change without a stage, so the event area is re-checked while the quest runs.
Event OnTimer(Int aiTimerID)
    If aiTimerID == 7631 && IsRunning()
        RefreshMusic(False)
        StartTimer(2.0, 7631)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(7631)
    RefreshMusic(True)
EndEvent

Bool Function PlayerHearsEventMusic()
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    Return eventQuest == None || eventQuest.IsPlayerParticipating()
EndFunction

Bool Function MusicDatumActive(Int aiIndex)
    MusicStageDatum datum = MusicStageData[aiIndex]
    If datum == None || datum.MusicTypeForm == None || !IsStageDone(datum.StageToStartMusic)
        Return False
    EndIf
    Return datum.StageToEndMusic <= 0 || !IsStageDone(datum.StageToEndMusic)
EndFunction

; Several rows can share one MusicType, so the form stays on the music stack while any of them still plays.
Bool Function MusicFormPlayingElsewhere(MusicType akMusic, Int aiExcludedIndex)
    Int index = 0
    While index < MusicStageData.Length
        If index != aiExcludedIndex && B21MusicPlaying[index] && MusicStageData[index] != None && MusicStageData[index].MusicTypeForm == akMusic
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function RefreshMusic(Bool abShuttingDown)
    If MusicStageData == None
        Return
    EndIf
    If B21MusicPlaying == None || B21MusicPlaying.Length != MusicStageData.Length
        B21MusicPlaying = new Bool[MusicStageData.Length]
    EndIf
    Bool hears = !abShuttingDown && PlayerHearsEventMusic()
    Int index = 0
    While index < MusicStageData.Length
        If B21MusicPlaying[index]
            MusicStageDatum datum = MusicStageData[index]
            Bool ended = !MusicDatumActive(index)
            ; PersistsOffQuest rows are one-shot tracks that finish on their own after the event.
            Bool leaving = abShuttingDown || !hears
            If ended || (leaving && !datum.PersistsOffQuest)
                B21MusicPlaying[index] = False
                If datum != None && datum.MusicTypeForm != None && !MusicFormPlayingElsewhere(datum.MusicTypeForm, index)
                    datum.MusicTypeForm.Remove()
                EndIf
            EndIf
        EndIf
        index += 1
    EndWhile
    If !hears
        Return
    EndIf
    index = 0
    While index < MusicStageData.Length
        If !B21MusicPlaying[index] && MusicDatumActive(index)
            If !MusicFormPlayingElsewhere(MusicStageData[index].MusicTypeForm, index)
                MusicStageData[index].MusicTypeForm.Add()
            EndIf
            B21MusicPlaying[index] = True
        EndIf
        index += 1
    EndWhile
EndFunction
