Event OnQuestInit()
    UpdateLocalCodeCycle()
    Initialized = True
    EnsureLocalOfficers()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        checkCodeResetBusy = False
        B21StartingOfficers = False
        UpdateLocalCodeCycle()
        EnsureLocalOfficers()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 76011
        EnsureLocalOfficers()
    EndIf
EndEvent

Bool Function EnsureLocalOfficers()
    Quest codesQuest = Game.GetFormFromFile(0x003DA647, "SeventySix.esm") as Quest
    Nuke_CodesScript codes = codesQuest as Nuke_CodesScript
    If codes == None
        Return False
    EndIf
    If B21StartingOfficers
        Return codesQuest.IsRunning()
    EndIf
    B21StartingOfficers = True
    If !codesQuest.IsRunning()
        EN07_NukeMasterScript launchMaster = EN07_MQ_Nuke_Master as EN07_NukeMasterScript
        If Nuke_CodesStartQuest != None && launchMaster != None && launchMaster.CodeData != None && launchMaster.CodeData.Length > 0
            Location siloLocation = launchMaster.CodeData[0].SiloLocation
            If siloLocation != None
                Actor player = Game.GetPlayer()
                Nuke_CodesStartQuest.SendStoryEventAndWait(siloLocation, player, player)
            EndIf
        EndIf
    EndIf
    If !codesQuest.IsRunning()
        B21StartingOfficers = False
        StartTimer(60.0, 76011)
        Return False
    EndIf
    CancelTimer(76011)
    codes.InitializeLocalOfficers()
    B21StartingOfficers = False
    Return True
EndFunction

Event OnTimerGameTime(Int aiTimerID)
    If aiTimerID == 76010
        UpdateLocalCodeCycle()
    EndIf
EndEvent

Function UpdateLocalCodeCycle()
    CancelTimerGameTime(76010)
    If checkCodeResetBusy || !B21:KeypadNative.Ready()
        Return
    EndIf
    checkCodeResetBusy = True
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    InitializeLocalCodeState()
    NukeCodesActive = True
    NukeCodesSolutionAvailable = True
    UpdateLocalCodePages(CodePagesBase00, 0, False)
    UpdateLocalCodePages(CodePagesBase01, 1, False)
    UpdateLocalCodePages(CodePagesBase02, 2, False)
    checkCodeResetBusy = False
EndFunction

Function InitializeLocalCodeState()
    If B21LocalCodeSeed <= 0
        B21LocalCodeSeed = Utility.RandomInt(1, 2147483646)
    EndIf
    If B21LocalCodeRevisions == None
        Int revision = CurrentCodeIndex
        If revision < 0
            revision = 0
        EndIf
        ; Preserve the code already issued by an earlier version of the local puzzle.
        B21LocalCodeRevisions = new Int[3]
        B21LocalCodeRevisions[0] = revision
        B21LocalCodeRevisions[1] = revision
        B21LocalCodeRevisions[2] = revision
    EndIf
    If B21LocalCodeUsed == None
        B21LocalCodeUsed = new Bool[3]
    EndIf
EndFunction

Int Function LocalCodeRevision(Int silo)
    If silo < 0 || silo > 2
        Return -1
    EndIf
    InitializeLocalCodeState()
    Return B21LocalCodeRevisions[silo]
EndFunction

Book Function LocalCodePage(Int silo, Int piece)
    Book[] pages = CodePagesBase00
    If silo == 1
        pages = CodePagesBase01
    ElseIf silo == 2
        pages = CodePagesBase02
    ElseIf silo != 0
        Return None
    EndIf
    If pages == None || piece < 0 || piece >= pages.Length || piece > 7
        Return None
    EndIf
    Return pages[piece]
EndFunction

Function MarkLocalCodeUsed(Int silo)
    If silo < 0 || silo > 2
        Return
    EndIf
    InitializeLocalCodeState()
    B21LocalCodeUsed[silo] = True
EndFunction

Function RenewLocalCodeAfterLaunch(Int silo)
    If silo < 0 || silo > 2
        Return
    EndIf
    InitializeLocalCodeState()
    If !B21LocalCodeUsed[silo]
        Return
    EndIf
    B21LocalCodeUsed[silo] = False
    B21LocalCodeRevisions[silo] += 1
    If silo == 0
        UpdateLocalCodePages(CodePagesBase00, silo, True)
    ElseIf silo == 1
        UpdateLocalCodePages(CodePagesBase01, silo, True)
    Else
        UpdateLocalCodePages(CodePagesBase02, silo, True)
    EndIf
    InvalidateLocalLaunchCode(silo)
    Debug.Notification("The silo has reset and issued a new launch cipher.")
EndFunction

Function UpdateLocalCodePages(Book[] pages, Int silo, Bool used)
    If pages == None
        Return
    EndIf
    Actor player = Game.GetPlayer()
    Int index = 0
    While index < pages.Length && index < 8
        If pages[index] != None
            If used && player.GetItemCount(pages[index]) > 0
                player.RemoveItem(pages[index], player.GetItemCount(pages[index]), True)
            EndIf
            B21:KeypadNative.RenamePiece(pages[index], B21LocalCodeSeed, LocalCodeRevision(silo), silo, index)
        EndIf
        index += 1
    EndWhile
EndFunction

String Function DescribeLocalCodePiece(Form piece)
    UpdateLocalCodeCycle()
    Int index = -1
    Int silo = 0
    If CodePagesBase00 != None
        index = CodePagesBase00.Find(piece as Book)
    EndIf
    If index < 0 && CodePagesBase01 != None
        index = CodePagesBase01.Find(piece as Book)
        silo = 1
    EndIf
    If index < 0 && CodePagesBase02 != None
        index = CodePagesBase02.Find(piece as Book)
        silo = 2
    EndIf
    If index < 0 || index > 7 || B21LocalCodeSeed <= 0
        Return "This code piece is unavailable."
    EndIf
    Return B21:KeypadNative.PieceText(B21LocalCodeSeed, LocalCodeRevision(silo), silo, index) + "\n\nCollect all eight pieces for this silo. The Whitespring command-wing cipher printer supplies the keyword and decoding instructions. This cipher remains valid until a successful launch and the silo's reset."
EndFunction

String Function LocalCipherBriefing(Int silo)
    UpdateLocalCodeCycle()
    Return B21:KeypadNative.Briefing(B21LocalCodeSeed, LocalCodeRevision(silo), silo, 0.0)
EndFunction

String[] Function LocalKeywordLetters(Int silo)
    UpdateLocalCodeCycle()
    Return B21:KeypadNative.KeywordLetters(B21LocalCodeSeed, LocalCodeRevision(silo), silo, 0.0)
EndFunction

Bool Function CheckLocalLaunchCode(Int silo, Int enteredCode, Int entryCycle)
    UpdateLocalCodeCycle()
    Return entryCycle == LocalCodeRevision(silo) && B21:KeypadNative.ValidateLaunchCode(B21LocalCodeSeed, LocalCodeRevision(silo), silo, enteredCode)
EndFunction

Function InvalidateLocalLaunchCode(Int siloID)
    EN07_NukeMasterScript launchMaster = EN07_MQ_Nuke_Master as EN07_NukeMasterScript
    If launchMaster == None || launchMaster.CodeData == None
        Return
    EndIf
    Int index = siloID + 3
    If index >= 3 && index < launchMaster.CodeData.Length
        EN07_NukeMasterScript:CodeDatum silo = launchMaster.CodeData[index]
        EN07_ExternalKeypadAliasScript keypad = silo.KeypadActive as EN07_ExternalKeypadAliasScript
        If keypad != None
            keypad.ResetLocalEntry()
        EndIf
        ObjectReference terminalRef = silo.TargetingComputerAlias.GetReference()
        If terminalRef != None
            terminalRef.BlockActivation(True, False)
        EndIf
    EndIf
EndFunction
