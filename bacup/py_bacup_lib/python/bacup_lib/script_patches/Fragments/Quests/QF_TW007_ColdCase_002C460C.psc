; FO76 stocked every clue item through server-side alias creation that did not survive
; conversion: the quest-object aliases below carry no fill, so their containers stay empty.
; Each stage now creates the clue in its bound container (or at its marker) and fills the
; alias the stage-setting alias scripts watch. Re-entry reuses a filled alias.
ObjectReference Function TW007_StockClue(ReferenceAlias akHolderAlias, Form akItem, ReferenceAlias akItemAlias)
    If akItemAlias && akItemAlias.GetReference()
        Return akItemAlias.GetReference()
    EndIf
    If !akHolderAlias || !akItem
        Return None
    EndIf
    ObjectReference holderRef = akHolderAlias.GetReference()
    If !holderRef
        Return None
    EndIf
    ObjectReference itemRef = holderRef.PlaceAtMe(akItem, 1, True)
    If !itemRef
        Return None
    EndIf
    If holderRef.GetBaseObject() is Container || holderRef is Actor
        holderRef.AddItem(itemRef, 1, True)
    EndIf
    If akItemAlias
        akItemAlias.ForceRefTo(itemRef)
    EndIf
    Return itemRef
EndFunction

Function Fragment_Stage_0001_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0099_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0225_Item_00()
    If IsStageDone(250)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    If IsStageDone(225)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(22, True)
    ObjectReference safeRef = SecurityOfficeSafe.GetReference()
    If safeRef && safeRef.IsLocked() && safeRef.GetLockLevel() > 100
        safeRef.Unlock()
    EndIf
    TW007_holotapecounter counter = (Self as Quest) as TW007_holotapecounter
    If counter
        counter.WatchClueHolotape(TW007_StockClue(SecurityOfficeSafe, SlideHolo, TrackerClueHolo))
        counter.WatchClueHolotape(TW007_StockClue(HolotapeStorage, MailHolo, MailClueHolo))
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    If IsStageDone(375)
        SetStage(400)
    Else
        SetStage(380)
    EndIf
EndFunction

Function Fragment_Stage_0375_Item_00()
    If IsStageDone(350)
        SetStage(400)
    Else
        SetStage(380)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(22, True)
    SetObjectiveDisplayed(24, True)
    SetObjectiveDisplayed(26, True)
    TW007_StockClue(TrackerContainer, TrackerBroken, TrackerAlias)
    TW007_StockClue(MailDropBox, POKey, KeyClue)
    TW007_StockClue(MailDropBox, MediaLetter, MediaClue)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(24, True)
    SetObjectiveDisplayed(100, True)
    If IsStageDone(550)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(26, True)
    If IsStageDone(500)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(24, True)
    SetObjectiveCompleted(26, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(50, True)
    TW007_StockClue(PO_Box, Evidence, PO_Evidence)
    TW007_StockClue(PO_Box, Evidence_HR, PO_HRLetter)
    If !IsStageDone(801)
        SetStage(801)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True)
    Note1Container.TryToEnable()
    Note2Container.TryToEnable()
    Note3Container.TryToEnable()
    TW007_StockClue(Note1Container, Note1, FreddyNote1)
    TW007_StockClue(Note2Container, Note2, FreddyNote2)
    TW007_StockClue(Note3Container, Note3, FreddyNote3)
    TW007_StockClue(LighterContainer, LighterObj, Lighter)
    TW007_StockClue(OtisPikeCorpse_Container, DiaryEnd, OtisPikeDiaryEnd)
EndFunction

Function Fragment_Stage_1100_Item_00()
    SetObjectiveCompleted(100, True)
EndFunction

Function Fragment_Stage_1200_Item_00()
    SetObjectiveCompleted(60, True)
    SetObjectiveDisplayed(65, True)
EndFunction

Function Fragment_Stage_1300_Item_00()
    TW007_PlayerScript playerScript = Alias_Player as TW007_PlayerScript
    If playerScript != None
        playerScript.ReconcileFreddyClues()
    EndIf
EndFunction

Function Fragment_Stage_1400_Item_00()
    SetObjectiveCompleted(65, True)
    SetObjectiveDisplayed(110, True)
    TW007_StockClue(FreddyNoteHomeLoc, FreddyHomeNoteObj, FreddyNoteHome)
EndFunction

Function Fragment_Stage_1600_Item_00()
    SetObjectiveCompleted(110, True)
    SetObjectiveDisplayed(80, True)
EndFunction

Function Fragment_Stage_1700_Item_00()
    SetObjectiveCompleted(80, True)
    Stop()
EndFunction

Function Fragment_Stage_2000_Item_00()
    Stop()
EndFunction
