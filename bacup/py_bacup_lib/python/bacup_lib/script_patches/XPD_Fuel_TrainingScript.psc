Event OnQuestInit()
    B21ResultsBook = None
    B21AdamThrow = 0
    B21Completed = False
EndEvent

Event OnQuestShutdown()
    CleanUpDaily()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    MarkProgramRunning(auiStageID)

    If auiStageID == 121
        EnterStoryDivider(300)
    ElseIf auiStageID == 122
        EnterStoryDivider(400)
    ElseIf auiStageID == 123
        EnterStoryDivider(500)
    ElseIf auiStageID == 124
        EnterStoryDivider(600)
    ElseIf auiStageID == 246 || auiStageID == 249
        MarkProgress(245)
    ElseIf auiStageID == 312 || auiStageID == 314 || auiStageID == 316
        MarkAdamAttack(True)
    ElseIf auiStageID == 313 || auiStageID == 315 || auiStageID == 317 || auiStageID == 318
        MarkAdamAttack(False)
    ElseIf auiStageID == 10000
        CleanUpDaily()
    EndIf
EndEvent

Function StartDaily()
    B21ResultsBook = None
    B21AdamThrow = 0
    B21Completed = False
    CancelTimer(1)
    StartTimer(3.0, 1)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || !IsRunning()
        Return
    EndIf

    If !IsStageDone(Stage_PlayerHasSpokenToOrlando)
        If PlayerCanReachOrlando()
            SetStage(Stage_PlayerHasSpokenToOrlando)
        EndIf
    ElseIf IsStageDone(4000) && !IsStageDone(9000)
        If PlayerCanReachOrlando()
            SetStage(9000)
        EndIf
    EndIf

    If IsRunning() && !IsStageDone(9000)
        StartTimer(3.0, 1)
    EndIf
EndEvent

Actor Function GetDailyPlayer()
    Actor player = None
    If myPlayer != None
        player = myPlayer.GetActorReference()
    EndIf
    If player == None
        player = Game.GetPlayer()
    EndIf
    Return player
EndFunction

; The converted FO76 dialogue with Orlando cannot run, so proximity stands in for
; "spoke to Orlando". When Orlando has no reference at all, standing inside the
; Whitespring Refuge is accepted instead so the daily never becomes unplayable.
Bool Function PlayerCanReachOrlando()
    Actor player = GetDailyPlayer()
    If player == None
        Return False
    EndIf

    ReferenceAlias orlandoAlias = GetAlias(3) as ReferenceAlias
    ObjectReference orlando = None
    If orlandoAlias != None
        orlando = orlandoAlias.GetReference()
    EndIf
    If orlando != None
        Return player.GetDistance(orlando) <= 384.0
    EndIf

    LocationAlias refugeAlias = GetAlias(1) as LocationAlias
    If refugeAlias == None
        Return False
    EndIf
    Location refuge = refugeAlias.GetLocation()
    Return refuge != None && player.GetCurrentLocation() == refuge
EndFunction

Function PickStory()
    Int story = Utility.RandomInt(0, 4)
    MarkProgress(120 + story)
EndFunction

; Only the Rad Ghosts entry on the simulator's main menu carries the bound
; menu-item mapping that sets stage 115, so the other four stories would leave
; "Use the terminal to start the simulator" displayed forever. The first choice
; inside any story proves the program is running; the five story dividers are
; set from stage 110 and do not count.
Function MarkProgramRunning(Int aiStageID)
    If aiStageID <= 200 || aiStageID >= 4000 || IsStageDone(115)
        Return
    EndIf
    If aiStageID == 300 || aiStageID == 400 || aiStageID == 500 || aiStageID == 600
        Return
    EndIf
    SetStage(115)
EndFunction

Function EnterStoryDivider(Int aiDivider)
    MarkProgress(aiDivider)
EndFunction

Function MarkProgress(Int aiStage)
    If aiStage > 0 && !IsStageDone(aiStage)
        SetStage(aiStage)
    EndIf
EndFunction

Function MarkAdamAttack(Bool abSucceeded)
    If abSucceeded
        MarkProgress(324)
    Else
        MarkProgress(325)
    EndIf
EndFunction

; ideal out of 2x ideal is the plain 50% roll; a story stage that recorded an
; earlier good choice narrows the range to 1.5x, which is the 66% the stage note
; on 531 describes.
Bool Function RollIdeal(Int aiIdeal, Bool abBoosted)
    Int ideal = aiIdeal
    If ideal <= 0
        ideal = 10
    EndIf
    Int ceiling = ideal * 2
    If abBoosted
        ceiling = (ideal * 3) / 2
    EndIf
    If ceiling <= ideal
        Return True
    EndIf
    Return Utility.RandomInt(1, ceiling) <= ideal
EndFunction

Function PickAdamThrow()
    B21AdamThrow = Utility.RandomInt(1, 3)
    MarkProgress(319 + B21AdamThrow)
EndFunction

Bool Function ResolveRockPaperScissors(Int aiPlayerThrow)
    Int adam = B21AdamThrow
    If adam < 1 || adam > 3
        adam = Utility.RandomInt(1, 3)
        B21AdamThrow = adam
    EndIf

    Bool won = (aiPlayerThrow == 1 && adam == 3) || (aiPlayerThrow == 2 && adam == 1) || (aiPlayerThrow == 3 && adam == 2)
    If won
        MarkProgress(330)
    Else
        MarkProgress(331)
    EndIf
    Return won
EndFunction

Function RollVacuum()
    If RollIdeal(10, False)
        MarkProgress(256)
    Else
        MarkProgress(257)
    EndIf
EndFunction

Function RollAmputation(Int aiIdeal)
    If RollIdeal(aiIdeal, IsStageDone(429))
        MarkProgress(427)
    Else
        MarkProgress(426)
    EndIf
EndFunction

Function ChooseDiagnosis()
    MarkProgress(432)
EndFunction

Function RollDiagnosis()
    If RollIdeal(10, IsStageDone(429))
        MarkProgress(430)
    Else
        MarkProgress(431)
    EndIf
EndFunction

Function RollReactorFix(Int aiIdeal)
    If RollIdeal(aiIdeal, IsStageDone(531))
        MarkProgress(527)
    EndIf
EndFunction

Function RollReactorKnob(Int aiIdeal)
    If RollIdeal(aiIdeal, IsStageDone(531))
        MarkProgress(534)
    Else
        MarkProgress(535)
    EndIf
EndFunction

Function RollThiefCall()
    If RollIdeal(10, IsStageDone(620))
        MarkProgress(625)
    Else
        MarkProgress(626)
    EndIf
EndFunction

Function RememberResults(Form akResults)
    B21ResultsBook = akResults
EndFunction

Function CompleteDaily()
    If B21Completed
        Return
    EndIf
    B21Completed = True
    CompleteAllObjectives()
    CancelTimer(1)
EndFunction

Function CleanUpDaily()
    CancelTimer(1)

    Actor player = GetDailyPlayer()
    If player != None && B21ResultsBook != None
        player.RemoveItem(B21ResultsBook, 1, True)
    EndIf
    B21ResultsBook = None
    B21AdamThrow = 0
    B21Completed = False

    ReferenceAlias resultsAlias = GetAlias(17) as ReferenceAlias
    If resultsAlias != None
        resultsAlias.Clear()
    EndIf
EndFunction
