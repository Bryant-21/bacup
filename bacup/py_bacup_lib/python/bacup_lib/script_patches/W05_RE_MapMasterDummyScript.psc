Event OnLoad()
    PlayerToCheck = Game.GetPlayer()
    MapSegment1 = GetLinkedRef(W05_RE_MapSegment1_Keyword)
    MapSegment2 = GetLinkedRef(W05_RE_MapSegment2_Keyword)
    MapSegment3 = GetLinkedRef(W05_RE_MapSegment3_Keyword)
    MapSegment4 = GetLinkedRef(W05_RE_MapSegment4_Keyword)
    MapSegment5 = GetLinkedRef(W05_RE_MapSegment5_Keyword)
    MapSegment6 = GetLinkedRef(W05_RE_MapSegment6_Keyword)

    If PlayerToCheck == None || W05_MQ00_CodeAV == None
        Return
    EndIf

    MapNumbers = PlayerToCheck.GetValue(W05_MQ00_CodeAV) as Int
    If MapNumbers < 100000 || MapNumbers > 999999
        MapNumbers = Utility.RandomInt(100000, 999999)
        PlayerToCheck.SetValue(W05_MQ00_CodeAV, MapNumbers)
    EndIf

    ShowMapNumber(MapSegment1, MapNumbers / 100000)
    ShowMapNumber(MapSegment2, (MapNumbers / 10000) % 10)
    ShowMapNumber(MapSegment3, (MapNumbers / 1000) % 10)
    ShowMapNumber(MapSegment4, (MapNumbers / 100) % 10)
    ShowMapNumber(MapSegment5, (MapNumbers / 10) % 10)
    ShowMapNumber(MapSegment6, MapNumbers % 10)
EndEvent

Event OnUnload()
    ClearMapNumber(MapSegment1)
    ClearMapNumber(MapSegment2)
    ClearMapNumber(MapSegment3)
    ClearMapNumber(MapSegment4)
    ClearMapNumber(MapSegment5)
    ClearMapNumber(MapSegment6)
EndEvent

Function ShowMapNumber(ObjectReference segmentReference, Int numberToShow)
    W05_RE_MapSegmentDummyScript segmentScript = segmentReference as W05_RE_MapSegmentDummyScript
    If segmentScript != None
        segmentScript.ShowNumber(numberToShow)
    EndIf
EndFunction

Function ClearMapNumber(ObjectReference segmentReference)
    W05_RE_MapSegmentDummyScript segmentScript = segmentReference as W05_RE_MapSegmentDummyScript
    If segmentScript != None
        segmentScript.ClearNumber()
    EndIf
EndFunction

Function RevealMapSegment(Int segmentIndex)
    If PlayerToCheck == None
        PlayerToCheck = Game.GetPlayer()
    EndIf
    If PlayerToCheck == None || W05_MQ00_CodeAV == None
        Return
    EndIf

    MapNumbers = PlayerToCheck.GetValue(W05_MQ00_CodeAV) as Int
    If MapNumbers < 100000 || MapNumbers > 999999
        MapNumbers = Utility.RandomInt(100000, 999999)
        PlayerToCheck.SetValue(W05_MQ00_CodeAV, MapNumbers)
    EndIf

    ObjectReference segmentReference = None
    Int numberToShow = 0
    If segmentIndex == 1
        MapSegment1 = GetLinkedRef(W05_RE_MapSegment1_Keyword)
        segmentReference = MapSegment1
        numberToShow = MapNumbers / 100000
    ElseIf segmentIndex == 2
        MapSegment2 = GetLinkedRef(W05_RE_MapSegment2_Keyword)
        segmentReference = MapSegment2
        numberToShow = (MapNumbers / 10000) % 10
    ElseIf segmentIndex == 3
        MapSegment3 = GetLinkedRef(W05_RE_MapSegment3_Keyword)
        segmentReference = MapSegment3
        numberToShow = (MapNumbers / 1000) % 10
    ElseIf segmentIndex == 4
        MapSegment4 = GetLinkedRef(W05_RE_MapSegment4_Keyword)
        segmentReference = MapSegment4
        numberToShow = (MapNumbers / 100) % 10
    ElseIf segmentIndex == 5
        MapSegment5 = GetLinkedRef(W05_RE_MapSegment5_Keyword)
        segmentReference = MapSegment5
        numberToShow = (MapNumbers / 10) % 10
    ElseIf segmentIndex == 6
        MapSegment6 = GetLinkedRef(W05_RE_MapSegment6_Keyword)
        segmentReference = MapSegment6
        numberToShow = MapNumbers % 10
    Else
        Return
    EndIf

    W05_RE_MapSegmentDummyScript segmentScript = segmentReference as W05_RE_MapSegmentDummyScript
    If segmentScript != None
        segmentScript.RevealMapSegment(numberToShow)
    EndIf
EndFunction
